#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

use super::util::{
    SessionMeta, canonicalize_existing_dir, canonicalize_existing_file, default_codex_sessions_dir,
    extract_message, infer_project_hint, is_within_root, normalize_snippet_whitespace,
    parse_session_meta,
};

fn format_capture_content(
    ref_path: &str,
    focus_line: Option<usize>,
    focus_byte: Option<u64>,
    entries: &[Value],
) -> String {
    let mut out = String::new();
    out.push_str("Transcript capture\n");
    match (focus_line, focus_byte) {
        (Some(line), _) => out.push_str(&format!("ref: {ref_path}#L{line}\n")),
        (None, Some(byte)) => out.push_str(&format!("ref: {ref_path}#B{byte}\n")),
        _ => out.push_str(&format!("ref: {ref_path}\n")),
    }
    out.push_str("takeaway: <fill>\n");

    let mut chosen: Vec<&Value> = Vec::new();
    let mut focus_idx: Option<usize> = None;
    for (idx, entry) in entries.iter().enumerate() {
        if let Some(line) = focus_line {
            if entry.get("line").and_then(|v| v.as_u64()) == Some(line as u64) {
                focus_idx = Some(idx);
                break;
            }
        } else if let Some(byte) = focus_byte
            && entry.get("byte").and_then(|v| v.as_u64()) == Some(byte)
        {
            focus_idx = Some(idx);
            break;
        }
    }
    if let Some(idx) = focus_idx {
        if idx > 0 {
            chosen.push(&entries[idx.saturating_sub(1)]);
        }
        chosen.push(&entries[idx]);
        if idx + 1 < entries.len() {
            chosen.push(&entries[idx + 1]);
        }
    } else {
        chosen.extend(entries.iter().take(2));
    }

    if !chosen.is_empty() {
        out.push_str("context:\n");
        for entry in chosen {
            let role = entry
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let text = entry.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let text = normalize_snippet_whitespace(text);
            if text.is_empty() {
                continue;
            }
            let snippet = truncate_string_bytes(&text, 360);
            out.push_str(&format!("- {role}: {snippet}\n"));
        }
    }

    truncate_string_bytes(out.trim_end(), 1400)
}

impl McpServer {
    pub(crate) fn tool_branchmind_transcripts_open(&mut self, args: Value) -> Value {
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
        let ref_obj = match args_obj.get("ref").and_then(|v| v.as_object()) {
            Some(v) => v,
            None => return ai_error("INVALID_INPUT", "ref is required"),
        };
        let ref_path = match ref_obj.get("path").and_then(|v| v.as_str()) {
            Some(v) => v.to_string(),
            None => return ai_error("INVALID_INPUT", "ref.path is required"),
        };
        let ref_line = ref_obj.get("line").and_then(|v| v.as_i64());
        let ref_byte = ref_obj.get("byte").and_then(|v| v.as_u64());
        match (ref_line, ref_byte) {
            (Some(line), None) => {
                if line < 1 {
                    return ai_error("INVALID_INPUT", "ref.line must be >= 1");
                }
            }
            (None, Some(_)) => {}
            (Some(_), Some(_)) => {
                return ai_error(
                    "INVALID_INPUT",
                    "ref must include exactly one of line or byte",
                );
            }
            (None, None) => {
                return ai_error("INVALID_INPUT", "ref.line or ref.byte is required");
            }
        }

        let before_lines = match optional_usize(args_obj, "before_lines") {
            Ok(v) => v.unwrap_or(8),
            Err(resp) => return resp,
        };
        let after_lines = match optional_usize(args_obj, "after_lines") {
            Ok(v) => v.unwrap_or(8),
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

        if PathBuf::from(&ref_path).is_absolute() {
            return ai_error("INVALID_INPUT", "ref.path must be a relative path");
        }

        let joined = root_dir.join(&ref_path);
        let file_path = match canonicalize_existing_file(&joined) {
            Ok(v) => v,
            Err(err) => {
                return ai_error("INVALID_INPUT", &format!("ref.path: {err}"));
            }
        };
        if !is_within_root(&root_dir, &file_path) {
            return ai_error("INVALID_INPUT", "ref.path resolves outside root_dir");
        }

        let mut meta = SessionMeta::default();
        let mut entries = Vec::new();
        let per_entry_max_bytes = 4096usize;
        let mut warnings: Vec<Value> = Vec::new();

        // Keep open() bounded even for huge files: line refs scan until end_line; byte refs read a fixed window.
        let mut focus_line: Option<usize> = None;
        let mut focus_byte: Option<u64> = None;

        if let Some(line_raw) = ref_line {
            // Line-addressed open (classic).
            let file = match std::fs::File::open(&file_path) {
                Ok(v) => v,
                Err(err) => return ai_error("IO_ERROR", &format!("open failed: {err}")),
            };
            let mut reader = BufReader::new(file);
            let mut line_buf = String::new();
            let mut line_no = 0usize;
            let mut byte_off = 0u64;

            let focus = line_raw as usize;
            focus_line = Some(focus);
            let start_line = focus.saturating_sub(before_lines);
            let end_line = focus.saturating_add(after_lines);

            loop {
                line_buf.clear();
                let line_start_off = byte_off;
                let bytes = match reader.read_line(&mut line_buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                byte_off = byte_off.saturating_add(bytes as u64);
                line_no += 1;

                if line_no == focus {
                    focus_byte = Some(line_start_off);
                }
                if line_no > end_line {
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
                    continue;
                }
                if line_no < start_line {
                    continue;
                }
                let Some(message) = extract_message(&value) else {
                    continue;
                };

                let text = truncate_string_bytes(&message.text, per_entry_max_bytes);
                entries.push(json!({
                    "line": line_no,
                    "byte": line_start_off,
                    "role": message.role,
                    "ts": message.ts,
                    "text": text
                }));
            }
        } else if let Some(byte) = ref_byte {
            // Byte-addressed open (stable for huge JSONL; avoids full-file line scans).
            focus_byte = Some(byte);

            // Best-effort: read session_meta from a small head window (keeps project hint stable).
            if let Ok(file) = std::fs::File::open(&file_path) {
                let mut reader = BufReader::new(file);
                let mut line_buf = String::new();
                let mut read_bytes = 0usize;
                let head_cap = 64 * 1024;
                while read_bytes < head_cap {
                    line_buf.clear();
                    let bytes = match reader.read_line(&mut line_buf) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    read_bytes = read_bytes.saturating_add(bytes);
                    let line = line_buf.trim_end_matches('\n').trim_end_matches('\r');
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(line) else {
                        continue;
                    };
                    if let Some(sm) = parse_session_meta(&value) {
                        meta = sm;
                        break;
                    }
                }
            }

            let mut file = match std::fs::File::open(&file_path) {
                Ok(v) => v,
                Err(err) => return ai_error("IO_ERROR", &format!("open failed: {err}")),
            };
            let meta_fs = match file.metadata() {
                Ok(v) => v,
                Err(err) => return ai_error("IO_ERROR", &format!("stat failed: {err}")),
            };
            let file_len = meta_fs.len();
            if file_len == 0 {
                return ai_error("NOT_FOUND", "transcript file is empty");
            }
            if byte >= file_len {
                return ai_error("INVALID_INPUT", "ref.byte is out of range for this file");
            }

            let wanted_lines = before_lines.saturating_add(after_lines).saturating_add(1);
            let mut window = (wanted_lines.saturating_mul(4096)).clamp(64 * 1024, 512 * 1024);
            let max_window = 2 * 1024 * 1024;

            #[derive(Clone, Copy)]
            struct LineSpan {
                abs_start: u64,
                start: usize,
                end: usize,
            }

            let mut spans: Vec<LineSpan> = Vec::new();
            let mut buf: Vec<u8> = Vec::new();

            for _attempt in 0..3 {
                let half = (window / 2) as u64;
                let start = byte.saturating_sub(half);
                let start = start.min(file_len.saturating_sub(1));
                let end = (start as usize)
                    .saturating_add(window)
                    .min(file_len as usize);
                let to_read = end.saturating_sub(start as usize);
                if to_read == 0 {
                    break;
                }
                if file.seek(SeekFrom::Start(start)).is_err() {
                    break;
                }

                buf = vec![0u8; to_read];
                let Ok(read) = file.read(&mut buf) else {
                    break;
                };
                buf.truncate(read);
                if buf.is_empty() {
                    break;
                }

                let mut bytes = buf.as_slice();
                let mut base_offset = start;
                let mut prefix_skip = 0usize;
                if start > 0 {
                    // Skip the first partial line (we started mid-file).
                    if let Some(pos) = bytes.iter().position(|b| *b == b'\n') {
                        let adv = (pos + 1).min(bytes.len());
                        base_offset = base_offset.saturating_add(adv as u64);
                        prefix_skip = adv;
                        bytes = &bytes[adv..];
                    }
                }

                spans.clear();
                let mut i = 0usize;
                while i <= bytes.len() {
                    let line_start = i;
                    let mut line_end = bytes.len();
                    if i < bytes.len()
                        && let Some(pos) = bytes[i..].iter().position(|b| *b == b'\n')
                    {
                        line_end = i.saturating_add(pos);
                    }
                    let mut end_trim = line_end;
                    if end_trim > line_start && bytes[end_trim - 1] == b'\r' {
                        end_trim = end_trim.saturating_sub(1);
                    }
                    spans.push(LineSpan {
                        abs_start: base_offset.saturating_add(line_start as u64),
                        start: prefix_skip.saturating_add(line_start),
                        end: prefix_skip.saturating_add(end_trim),
                    });

                    if i >= bytes.len() {
                        break;
                    }
                    i = line_end.saturating_add(1);
                }

                let exact_idx = spans.iter().position(|s| s.abs_start == byte);
                if exact_idx.is_some() {
                    break;
                }

                if window >= max_window {
                    break;
                }
                window = (window.saturating_mul(2)).min(max_window);
            }

            if spans.is_empty() {
                return ai_error("NOT_FOUND", "no transcript lines found near ref.byte");
            }

            let mut focus_idx = spans.iter().position(|s| s.abs_start == byte);
            if focus_idx.is_none() {
                // Deterministic fallback: pick the nearest preceding line start.
                let mut best: Option<usize> = None;
                for (idx, span) in spans.iter().enumerate() {
                    if span.abs_start <= byte {
                        best = Some(idx);
                    } else {
                        break;
                    }
                }
                focus_idx = best.or(Some(0));
                warnings.push(warning(
                    "TRANSCRIPTS_REF_NOT_EXACT",
                    "ref.byte did not match an exact line start; opening nearest preceding line.",
                    "Use transcripts_digest to regenerate refs, or retry with a larger window.",
                ));
            }

            let focus_idx = focus_idx.unwrap_or(0);
            let start_idx = focus_idx.saturating_sub(before_lines);
            let end_idx =
                (focus_idx.saturating_add(after_lines)).min(spans.len().saturating_sub(1));

            for span in spans
                .iter()
                .skip(start_idx)
                .take(end_idx.saturating_sub(start_idx).saturating_add(1))
            {
                let line = &buf[span.start..span.end];
                let Ok(value) = serde_json::from_slice::<Value>(line) else {
                    continue;
                };
                if let Some(sm) = parse_session_meta(&value) {
                    meta = sm;
                    continue;
                }
                let Some(message) = extract_message(&value) else {
                    continue;
                };
                let text = truncate_string_bytes(&message.text, per_entry_max_bytes);
                entries.push(json!({
                    "byte": span.abs_start,
                    "role": message.role,
                    "ts": message.ts,
                    "text": text
                }));
            }
        }

        let project = infer_project_hint(&meta, &format!("file:{ref_path}"));
        let entries_snapshot = entries.clone();
        let mut ref_obj = json!({ "path": ref_path });
        if let Some(line) = focus_line {
            ref_obj["line"] = json!(line);
        }
        if let Some(byte) = focus_byte {
            ref_obj["byte"] = json!(byte);
        }
        let mut result = json!({
            "workspace": workspace.as_str(),
            "root_dir": root_dir_raw,
            "ref": ref_obj,
            "session": {
                "id": meta.id,
                "ts": meta.ts,
                "cwd": meta.cwd
            },
            "project": {
                "id": project.id,
                "name": project.name,
                "confidence": project.confidence
            },
            "entries": entries_snapshot,
            "truncated": false
        });

        redact_value(&mut result, 6);

        // Manual ROI hook: suggest a capture into durable BranchMind notes (still read-only until executed).
        // Keep this low-noise: one best action (personal lane) + one backup (shared lane), only when
        // the portal tool exists in the current toolset.
        let mut suggestions = Vec::new();
        if matches!(self.toolset, Toolset::Daily | Toolset::Full) {
            let capture_content =
                format_capture_content(&ref_path, focus_line, focus_byte, &entries);

            let mut transcript_ref = json!({ "path": ref_path });
            if let Some(line) = focus_line {
                transcript_ref["line"] = json!(line);
            }
            if let Some(byte) = focus_byte {
                transcript_ref["byte"] = json!(byte);
            }
            let mut capture_meta = json!({
                "source": { "kind": "transcripts", "tool": "transcripts_open" },
                "transcript": {
                    "ref": transcript_ref,
                    "session": { "id": meta.id, "ts": meta.ts, "cwd": meta.cwd },
                    "project": { "id": project.id, "name": project.name, "confidence": project.confidence }
                }
            });

            // Step-aware grafting: if the workspace focus is a TASK with an open step,
            // attach meta.step so step-scoped views pick it up naturally.
            if let Ok(Some(focus)) = self.store.focus_get(&workspace)
                && focus.starts_with("TASK-")
                && let Ok(summary) = self.store.task_steps_summary(&workspace, &focus)
                && let Some(first_open) = summary.first_open
                && let Some(obj) = capture_meta.as_object_mut()
            {
                obj.insert(
                    "step".to_string(),
                    json!({
                        "task_id": focus,
                        "step_id": first_open.step_id,
                        "path": first_open.path
                    }),
                );
            }

            suggestions.push(suggest_call(
                "macro_branch_note",
                "Capture this transcript window into notes (personal lane).",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "title": "Transcript capture",
                    "format": "text",
                    "meta": capture_meta.clone(),
                    "content": capture_content
                }),
            ));

            // Backup: shared lane capture (agent_id=null disables default agent_id injection).
            suggestions.push(suggest_call(
                "macro_branch_note",
                "Capture into notes (shared lane) for a durable team anchor.",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "agent_id": Value::Null,
                    "title": "Transcript capture (shared)",
                    "format": "text",
                    "meta": capture_meta,
                    "content": capture_content
                }),
            ));
        }

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= trim_array_to_budget(value, &["entries"], limit, false);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["session"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["ref"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["project"]);
                    }
                    if json_len_chars(value) > limit && retain_one_at(value, &["entries"], false) {
                        changed = true;
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings.extend(budget_warnings(truncated, minimal, clamped));
        }

        if warnings.is_empty() {
            ai_ok_with("transcripts_open", result, suggestions)
        } else {
            ai_ok_with_warnings("transcripts_open", result, warnings, suggestions)
        }
    }
}
