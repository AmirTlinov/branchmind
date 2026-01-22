#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::util::{
    SessionMeta, canonicalize_existing_dir, default_codex_sessions_dir, extract_message,
    extract_path_hints_from_text, list_jsonl_files_deterministic, parse_session_meta,
    project_cwd_prefix_from_storage_dir, session_matches_cwd_prefix,
};

#[derive(Clone, Debug)]
struct CandidateItem {
    sort_key: String,
    rel_path: String,
    session_id: Option<String>,
    session_ts: Option<String>,
    msg_ts: Option<String>,
    ref_line: Option<usize>,
    ref_byte: u64,
    text: String,
}

fn compact_digest_texts(value: &mut Value, max_bytes_per_text: usize) -> bool {
    if max_bytes_per_text == 0 {
        return false;
    }
    let Some(digest) = value.get_mut("digest").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    if digest.is_empty() {
        return false;
    }

    let mut changed = false;
    for item in digest.iter_mut() {
        let Some(msg) = item.get_mut("message").and_then(|v| v.as_object_mut()) else {
            continue;
        };
        let Some(Value::String(text)) = msg.get_mut("text") else {
            continue;
        };
        let new_text = truncate_string_bytes(text, max_bytes_per_text);
        if new_text != *text {
            *text = new_text;
            changed = true;
        }
    }
    changed
}

impl McpServer {
    pub(crate) fn tool_branchmind_transcripts_digest(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let root_dir_raw = match optional_string(args_obj, "root_dir") {
            Ok(v) => v.unwrap_or_else(default_codex_sessions_dir),
            Err(resp) => return resp,
        };
        let cwd_prefix_raw = match optional_string(args_obj, "cwd_prefix") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mode = match optional_string(args_obj, "mode") {
            Ok(v) => v.unwrap_or_else(|| "summary".to_string()),
            Err(resp) => return resp,
        };
        if mode != "summary" && mode != "last" {
            return ai_error("INVALID_INPUT", "mode must be 'summary' or 'last'");
        }

        let max_files = match optional_usize(args_obj, "max_files") {
            Ok(v) => v.unwrap_or(720),
            Err(resp) => return resp,
        };
        let max_bytes_total = match optional_usize(args_obj, "max_bytes_total") {
            Ok(v) => v.unwrap_or(16 * 1024 * 1024),
            Err(resp) => return resp,
        };
        let max_items = match optional_usize(args_obj, "max_items") {
            Ok(v) => v.unwrap_or(6),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let root_dir_path = PathBuf::from(&root_dir_raw);
        let root_dir = match canonicalize_existing_dir(&root_dir_path) {
            Ok(v) => v,
            Err(err) => {
                return ai_error("INVALID_INPUT", &format!("root_dir: {err}"));
            }
        };

        let cwd_prefix = cwd_prefix_raw
            .unwrap_or_else(|| project_cwd_prefix_from_storage_dir(self.store.storage_dir()));
        let require_meta_for_scan = !cwd_prefix.is_empty();

        let files = list_jsonl_files_deterministic(&root_dir, max_files.max(1));

        // Design goal: cover many sessions under a small scan budget.
        // Strategy:
        // 1) Read a small head slice per file to resolve project scoping (session_meta + early hints).
        // 2) Read a small tail slice per file to pick 0..1 candidates (summary / last assistant).
        // 3) Return byte-offset refs (stable for immutable JSONL). For small files where the tail scan
        //    covers the full file, we also include `ref.line` for convenience.
        let mut candidates: Vec<CandidateItem> = Vec::new();
        let mut seen = HashSet::<String>::new();
        let mut scanned_files = 0usize;
        let mut scanned_bytes = 0usize;
        let mut scan_truncated = false;

        let head_limit_bytes = 16 * 1024;
        let tail_limit_bytes = 128 * 1024;

        for file_path in files {
            if scanned_bytes >= max_bytes_total {
                scan_truncated = true;
                break;
            }

            scanned_files += 1;
            let rel_path = relativize_path(&root_dir, &file_path);

            let (meta, file_allowed) = scan_head_meta_and_scope(
                &file_path,
                require_meta_for_scan,
                &cwd_prefix,
                head_limit_bytes,
                &mut scanned_bytes,
                max_bytes_total,
                &mut scan_truncated,
            );

            if require_meta_for_scan && !file_allowed {
                continue;
            }

            let Some(candidate) = scan_tail_pick_candidate(
                &file_path,
                &mode,
                tail_limit_bytes,
                &mut scanned_bytes,
                max_bytes_total,
                &mut scan_truncated,
            ) else {
                continue;
            };

            let match_hash = stable_match_hash(&candidate.text);
            if !seen.insert(match_hash.clone()) {
                continue;
            }

            let sort_key = candidate
                .ts
                .clone()
                .or_else(|| meta.ts.clone())
                .unwrap_or_else(|| rel_path.clone());
            candidates.push(CandidateItem {
                sort_key,
                rel_path: rel_path.clone(),
                session_id: meta.id.clone(),
                session_ts: meta.ts.clone(),
                msg_ts: candidate.ts.clone(),
                ref_line: candidate.line,
                ref_byte: candidate.byte,
                text: truncate_string_bytes(&candidate.text, 12_000),
            });

            if candidates.len() >= max_items.saturating_mul(3) {
                // Avoid over-collecting; we will sort+trim and then resolve line refs for the winners.
                break;
            }
        }

        candidates.sort_by(|a, b| {
            b.sort_key
                .cmp(&a.sort_key)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });

        let mut digest = Vec::new();
        for candidate in candidates {
            if digest.len() >= max_items.max(1) {
                break;
            }

            let mut ref_obj = json!({
                "path": candidate.rel_path,
                "byte": candidate.ref_byte
            });
            if let Some(line_no) = candidate.ref_line
                && let Some(obj) = ref_obj.as_object_mut()
            {
                obj.insert("line".to_string(), json!(line_no));
            }

            // Note: We intentionally avoid resolving `ref.line` by scanning from file start.
            // Huge JSONL transcripts can be 100MB+, and line-resolution breaks the scan budget.
            // `ref.byte` is stable and allows `transcripts_open` to work without full scans.
            digest.push(json!({
                "ref": ref_obj,
                "session": { "id": candidate.session_id, "ts": candidate.session_ts },
                "message": { "role": "assistant", "ts": candidate.msg_ts, "text": candidate.text }
            }));
        }

        let digest_total = digest.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "root_dir": root_dir_raw.clone(),
            "filters": { "cwd_prefix": cwd_prefix, "mode": mode },
            "scanned": { "files": scanned_files, "bytes": scanned_bytes, "truncated": scan_truncated },
            "digest": digest,
            "truncated": false
        });

        redact_value(&mut result, 6);

        let mut suggestions = Vec::new();
        if let Some(first) = result
            .get("digest")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
        {
            let ref_obj = first.get("ref").and_then(|v| v.as_object());
            let path = ref_obj.and_then(|o| o.get("path")).and_then(|v| v.as_str());
            let line = ref_obj.and_then(|o| o.get("line")).and_then(|v| v.as_u64());
            let byte = ref_obj.and_then(|o| o.get("byte")).and_then(|v| v.as_u64());
            if let Some(path) = path {
                let mut open_ref = json!({ "path": path });
                if let Some(line) = line {
                    open_ref["line"] = json!(line);
                } else if let Some(byte) = byte {
                    open_ref["byte"] = json!(byte);
                }

                if open_ref.get("line").is_some() || open_ref.get("byte").is_some() {
                    suggestions.push(suggest_call(
                        "transcripts_open",
                        "Open the newest digest item for surrounding context (bounded).",
                        "low",
                        json!({
                            "workspace": workspace.as_str(),
                            "root_dir": root_dir_raw,
                            "ref": open_ref
                        }),
                    ));
                }
            }
        }

        let mut warnings = Vec::new();
        if digest_total == 0 && scan_truncated {
            warnings.push(warning(
                "TRANSCRIPTS_SCAN_TRUNCATED",
                "No digest items were found under the scan budget.",
                "Increase max_bytes_total/max_files or retry with a different mode (e.g. mode='last').",
            ));

            let bump_bytes = if max_bytes_total < 64 * 1024 * 1024 {
                64 * 1024 * 1024
            } else {
                (max_bytes_total.saturating_mul(2)).min(256 * 1024 * 1024)
            };
            let bump_files = if max_files < 120 {
                120
            } else {
                (max_files * 2).min(2048)
            };

            suggestions.push(suggest_call(
                "transcripts_digest",
                "Retry with a larger scan budget (best-effort, still bounded).",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "root_dir": root_dir_raw.clone(),
                    "cwd_prefix": cwd_prefix.clone(),
                    "mode": mode,
                    "max_items": max_items,
                    "max_files": bump_files,
                    "max_bytes_total": bump_bytes
                }),
            ));

            if mode == "summary" {
                suggestions.push(suggest_call(
                    "transcripts_digest",
                    "If you just need orientation, try mode='last' (often returns something faster).",
                    "low",
                    json!({
                        "workspace": workspace.as_str(),
                        "root_dir": root_dir_raw.clone(),
                        "cwd_prefix": cwd_prefix.clone(),
                        "mode": "last",
                        "max_items": max_items
                    }),
                ));
            }
        }
        if digest_total == 0
            && !scan_truncated
            && scanned_files >= max_files.max(1)
            && scanned_bytes < max_bytes_total
        {
            warnings.push(warning(
                "TRANSCRIPTS_MAX_FILES_REACHED",
                "No digest items were found in the scanned file set.",
                "Increase max_files (or switch to mode='last') to scan older sessions for this project.",
            ));

            suggestions.push(suggest_call(
                "transcripts_digest",
                "Retry scanning more files (still bounded by max_bytes_total).",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "root_dir": root_dir_raw.clone(),
                    "cwd_prefix": cwd_prefix.clone(),
                    "mode": mode,
                    "max_items": max_items,
                    "max_files": (max_files.saturating_mul(2)).clamp(240, 2048),
                    "max_bytes_total": max_bytes_total
                }),
            ));
        }
        if digest_total == 0 && mode == "summary" {
            // Even when scan budgets are fine, "summary" may simply be too strict for a session.
            // Offer a low-priority fallback that stays project-scoped.
            suggestions.push(suggest_call(
                "transcripts_digest",
                "Try mode='last' if you want the most recent assistant message per session.",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "root_dir": root_dir_raw.clone(),
                    "cwd_prefix": cwd_prefix.clone(),
                    "mode": "last",
                    "max_items": max_items
                }),
            ));
        }

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated_budget = false;
            let mut minimal = false;

            let _used = ensure_budget_limit(
                &mut result,
                limit,
                &mut truncated_budget,
                &mut minimal,
                |v| {
                    let mut changed = false;
                    if json_len_chars(v) > limit {
                        let bytes_per_text = if limit <= 1500 {
                            240
                        } else if limit <= 4000 {
                            800
                        } else {
                            1600
                        };
                        changed |= compact_digest_texts(v, bytes_per_text);
                    }
                    if json_len_chars(v) > limit {
                        changed |= trim_array_to_budget(v, &["digest"], limit, false);
                    }
                    if json_len_chars(v) > limit {
                        changed |= ensure_minimal_list_at(v, &["digest"], digest_total, "digest");
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["scanned"], &["bytes"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["scanned"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["filters"]);
                    }
                    changed
                },
            );

            set_truncated_flag(&mut result, truncated_budget);
            warnings.extend(budget_warnings(truncated_budget, minimal, clamped));
        }

        if warnings.is_empty() {
            ai_ok_with("transcripts_digest", result, suggestions)
        } else {
            ai_ok_with_warnings("transcripts_digest", result, warnings, suggestions)
        }
    }
}

fn relativize_path(root_dir: &Path, file_path: &Path) -> String {
    file_path
        .strip_prefix(root_dir)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string_lossy().to_string())
}

fn stable_msg_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn normalize_digest_text(text: &str) -> String {
    // Reduce accidental dedupe misses due to whitespace.
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        let is_space = ch.is_whitespace();
        if is_space {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            last_space = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn looks_like_summary(text: &str) -> bool {
    let lower = text.to_lowercase();
    // Strong markers: accept even for short messages.
    if [
        "итог:",
        "**итог:**",
        "итог —",
        "итоги:",
        "резюме:",
        "summary:",
        "tl;dr",
    ]
    .iter()
    .any(|m| lower.contains(m))
    {
        return true;
    }

    // Structured update patterns (low-noise): require multiple section headings.
    let mut sections = 0usize;
    for marker in [
        "статус:",
        "**статус:**",
        "изменилось:",
        "зачем:",
        "курс:",
        "риски:",
        "дальше",
        "нужно от тебя",
    ] {
        if lower.contains(marker) {
            sections += 1;
        }
    }

    if sections >= 3 {
        return true;
    }
    sections >= 2 && text.len() >= 180 && text.contains('\n')
}

#[derive(Clone, Debug)]
struct CandidateMessage {
    ts: Option<String>,
    text: String,
    byte: u64,
    line: Option<usize>,
}

fn stable_match_hash(text: &str) -> String {
    // Use a bounded, normalized form to keep matching deterministic and cheap even for huge messages.
    let normalized = normalize_digest_text(text);
    let bounded = truncate_string_bytes(&normalized, 4096);
    stable_msg_hash(&bounded)
}

fn scan_head_meta_and_scope(
    file_path: &Path,
    require_meta_for_scan: bool,
    cwd_prefix: &str,
    head_limit_bytes: usize,
    scanned_bytes: &mut usize,
    max_bytes_total: usize,
    scan_truncated: &mut bool,
) -> (SessionMeta, bool) {
    let mut meta = SessionMeta::default();
    let mut file_allowed = !require_meta_for_scan;
    if head_limit_bytes == 0 {
        return (meta, file_allowed);
    }

    let Ok(file) = std::fs::File::open(file_path) else {
        return (meta, file_allowed);
    };
    let mut reader = BufReader::new(file);
    let mut line_buf = String::new();
    let mut head_bytes = 0usize;
    let mut limit_bytes = head_limit_bytes;

    while head_bytes < head_limit_bytes && *scanned_bytes < max_bytes_total {
        line_buf.clear();
        let bytes = match reader.read_line(&mut line_buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        head_bytes = head_bytes.saturating_add(bytes);
        *scanned_bytes = scanned_bytes.saturating_add(bytes);
        if *scanned_bytes > max_bytes_total {
            *scan_truncated = true;
            break;
        }

        let line = line_buf.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        if let Some(sm) = parse_session_meta(&value) {
            meta = sm;
            if require_meta_for_scan {
                file_allowed = session_matches_cwd_prefix(&meta, cwd_prefix);
                if file_allowed {
                    break;
                }
                if let Some(cwd) = meta.cwd.as_deref()
                    && is_likely_project_path(cwd)
                {
                    // Session meta points to a concrete project directory that doesn't match this project.
                    // Still scan a tiny window for early <cwd> hints in case session_meta.cwd is stale.
                    limit_bytes = limit_bytes.min(head_bytes.saturating_add(4 * 1024));
                }
            } else {
                // When not scoping by project, session_meta is enough to populate session fields.
                // Keep scanning bounded to preserve the global scan budget.
                break;
            }
            continue;
        }

        if head_bytes >= limit_bytes {
            break;
        }

        if !require_meta_for_scan || file_allowed {
            continue;
        }

        let Some(message) = extract_message(&value) else {
            continue;
        };
        if message.role != "user" && message.role != "developer" {
            continue;
        }

        let hints = extract_path_hints_from_text(&message.text);
        if hints.is_empty() {
            continue;
        }
        meta.path_hints.extend(hints);
        meta.path_hints.sort();
        meta.path_hints.dedup();
        file_allowed = session_matches_cwd_prefix(&meta, cwd_prefix);
        if file_allowed {
            break;
        }
    }

    (meta, file_allowed)
}

fn is_likely_project_path(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = PathBuf::from(trimmed);
    let canon = std::fs::canonicalize(&path).unwrap_or(path);
    if !canon.is_dir() {
        return false;
    }

    // Bounded, deterministic heuristic: a few parent levels for common repo markers.
    let mut current: Option<&Path> = Some(canon.as_path());
    for _ in 0..=3 {
        let Some(dir) = current else {
            break;
        };
        if has_dir_marker(dir, ".git") || has_file_marker(dir, ".git") {
            return true;
        }
        if has_file_marker(dir, "Cargo.toml")
            || has_file_marker(dir, "package.json")
            || has_file_marker(dir, "pyproject.toml")
            || has_file_marker(dir, "go.mod")
        {
            return true;
        }
        current = dir.parent();
    }
    false
}

fn has_dir_marker(dir: &Path, name: &str) -> bool {
    let marker = dir.join(name);
    std::fs::metadata(marker)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

fn has_file_marker(dir: &Path, name: &str) -> bool {
    let marker = dir.join(name);
    std::fs::metadata(marker)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

fn scan_tail_pick_candidate(
    file_path: &Path,
    mode: &str,
    tail_limit_bytes: usize,
    scanned_bytes: &mut usize,
    max_bytes_total: usize,
    scan_truncated: &mut bool,
) -> Option<CandidateMessage> {
    if tail_limit_bytes == 0 {
        return None;
    }
    if *scanned_bytes >= max_bytes_total {
        *scan_truncated = true;
        return None;
    }

    // Two-phase tail scan:
    // - First pass: small window (fast path).
    // - Second pass: bigger window when the file tail is dominated by tool/non-message records.
    // This keeps typical cases fast while rescuing huge sessions where the last assistant message
    // is not in the last N bytes.
    let deep_tail_bytes = tail_limit_bytes
        .saturating_mul(8)
        .min(1024 * 1024)
        .max(tail_limit_bytes);

    let mut attempt = 0usize;
    while attempt < 2 {
        attempt += 1;
        let want = if attempt == 1 {
            tail_limit_bytes
        } else {
            deep_tail_bytes
        };

        if *scanned_bytes >= max_bytes_total {
            *scan_truncated = true;
            return None;
        }

        let Ok(mut file) = std::fs::File::open(file_path) else {
            return None;
        };
        let Ok(meta) = file.metadata() else {
            return None;
        };
        let file_len = meta.len() as usize;
        if file_len == 0 {
            return None;
        }

        let remaining = max_bytes_total.saturating_sub(*scanned_bytes);
        if remaining == 0 {
            *scan_truncated = true;
            return None;
        }

        let to_read = want.min(file_len).min(remaining);
        if to_read == 0 {
            return None;
        }
        let start = file_len.saturating_sub(to_read);
        if file.seek(SeekFrom::Start(start as u64)).is_err() {
            return None;
        }

        let mut buf = vec![0u8; to_read];
        let Ok(read) = file.read(&mut buf) else {
            return None;
        };
        buf.truncate(read);
        *scanned_bytes = scanned_bytes.saturating_add(read);
        if *scanned_bytes > max_bytes_total {
            *scan_truncated = true;
            return None;
        }

        let mut bytes = buf.as_slice();
        let mut base_offset = start as u64;
        let can_compute_line_no = start == 0;
        if start > 0 {
            // Skip the first partial line (we started mid-file).
            if let Some(pos) = bytes.iter().position(|b| *b == b'\n') {
                let advance = (pos + 1).min(bytes.len());
                base_offset = base_offset.saturating_add(advance as u64);
                bytes = &bytes[advance..];
            } else {
                // No line boundary in this window. If this was the fast pass, retry with the deep pass.
                // Otherwise give up (bounded).
                if attempt == 1 {
                    continue;
                }
                return None;
            }
        }

        fn trim_ascii_ws(mut s: &[u8]) -> &[u8] {
            while let Some(b) = s.first() {
                if b.is_ascii_whitespace() {
                    s = &s[1..];
                } else {
                    break;
                }
            }
            while let Some(b) = s.last() {
                if b.is_ascii_whitespace() {
                    s = &s[..s.len().saturating_sub(1)];
                } else {
                    break;
                }
            }
            s
        }

        let mut picked: Option<CandidateMessage> = None;
        let mut assistant_seen = false;
        let mut idx = 0usize;
        let mut line_no = 1usize;
        while idx <= bytes.len() {
            let line_start = idx;
            let mut line_end = bytes.len();
            if idx < bytes.len()
                && let Some(pos) = bytes[idx..].iter().position(|b| *b == b'\n')
            {
                line_end = idx.saturating_add(pos);
            }

            let mut line = &bytes[line_start..line_end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len().saturating_sub(1)];
            }
            let line_trimmed = trim_ascii_ws(line);
            if !line_trimmed.is_empty()
                && let Ok(value) = serde_json::from_slice::<Value>(line_trimmed)
                && let Some(message) = extract_message(&value)
                && message.role == "assistant"
            {
                assistant_seen = true;
                if mode != "summary" || looks_like_summary(&message.text) {
                    let abs_byte = base_offset.saturating_add(line_start as u64);
                    let abs_line = if can_compute_line_no {
                        Some(line_no)
                    } else {
                        None
                    };
                    picked = Some(CandidateMessage {
                        ts: message.ts.clone(),
                        text: message.text,
                        byte: abs_byte,
                        line: abs_line,
                    });
                }
            }

            if idx >= bytes.len() {
                break;
            }
            if can_compute_line_no {
                line_no = line_no.saturating_add(1);
            }
            idx = line_end.saturating_add(1);
        }

        if picked.is_some() {
            return picked;
        }
        if assistant_seen {
            // In summary mode: we saw assistants but no summary markers in this window.
            // Don't deep-scan further (bounded + deterministic).
            return None;
        }
        // No assistant messages in this tail window (common when the tail is tool output noise).
        // Retry once with a deeper window.
    }

    None
}
