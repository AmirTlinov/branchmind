#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::util::{
    ExtractedMessage, ProjectHint, SessionMeta, canonicalize_existing_dir,
    default_codex_sessions_dir, extract_message, extract_path_hints_from_text, infer_project_hint,
    list_jsonl_files_deterministic, normalize_snippet_whitespace, parse_session_meta,
    project_cwd_prefix_from_storage_dir, session_matches_cwd_prefix, slice_around_match,
};

impl McpServer {
    pub(crate) fn tool_branchmind_transcripts_search(&mut self, args: Value) -> Value {
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
        let query_raw = match require_string(args_obj, "query") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let query = query_raw.trim();
        if query.is_empty() {
            return ai_error("INVALID_INPUT", "query must not be empty");
        }

        let cwd_prefix_raw = match optional_string(args_obj, "cwd_prefix") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let role = match optional_string(args_obj, "role") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_files = match optional_usize(args_obj, "max_files") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_bytes_total = match optional_usize(args_obj, "max_bytes_total") {
            Ok(v) => v.unwrap_or(64 * 1024 * 1024),
            Err(resp) => return resp,
        };
        let hits_limit = match optional_usize(args_obj, "hits_limit") {
            Ok(v) => v.unwrap_or(25),
            Err(resp) => return resp,
        };
        let context_chars = match optional_usize(args_obj, "context_chars") {
            Ok(v) => v.unwrap_or(240),
            Err(resp) => return resp,
        };
        let dedupe = match optional_bool(args_obj, "dedupe") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let root_dir_path = PathBuf::from(&root_dir_raw);
        let root_dir = match canonicalize_existing_dir(&root_dir_path) {
            Ok(v) => v,
            Err(err) => return ai_error("INVALID_INPUT", &format!("root_dir: {err}")),
        };

        let cwd_prefix = cwd_prefix_raw
            .unwrap_or_else(|| project_cwd_prefix_from_storage_dir(self.store.storage_dir()));
        let require_meta_for_scan = !cwd_prefix.is_empty();

        let files = list_jsonl_files_deterministic(&root_dir, max_files.max(1));

        let mut hits = Vec::new();
        let mut seen_hits = HashSet::<String>::new();
        let mut project_summary = BTreeMap::<String, Value>::new();
        let mut scanned_files = 0usize;
        let mut scanned_bytes = 0usize;
        let mut scan_truncated = false;

        for file_path in files {
            if scanned_bytes >= max_bytes_total {
                scan_truncated = true;
                break;
            }
            let file = match std::fs::File::open(&file_path) {
                Ok(v) => v,
                Err(_) => continue,
            };
            scanned_files += 1;

            let rel_path = relativize_path(&root_dir, &file_path);
            let fallback_id_seed = format!("file:{rel_path}");

            let mut meta = SessionMeta::default();
            let mut file_allowed = !require_meta_for_scan;
            let mut file_project: Option<ProjectHint> = None;
            let mut project_counted = false;

            let mut reader = BufReader::new(file);
            let mut line_buf = String::new();
            let mut line_no = 0usize;
            let mut scanned_bytes_in_file = 0usize;
            let hint_scan_limit_bytes = 128 * 1024;

            loop {
                line_buf.clear();
                let bytes = match reader.read_line(&mut line_buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                line_no += 1;
                scanned_bytes = scanned_bytes.saturating_add(bytes);
                scanned_bytes_in_file = scanned_bytes_in_file.saturating_add(bytes);
                if scanned_bytes > max_bytes_total {
                    scan_truncated = true;
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
                        file_allowed = session_matches_cwd_prefix(&meta, &cwd_prefix);
                    }
                    if file_allowed && !project_counted {
                        let hint = infer_project_hint(&meta, &fallback_id_seed);
                        upsert_project_summary(&mut project_summary, &hint, true, false);
                        file_project = Some(hint);
                        project_counted = true;
                    }
                    continue;
                }

                let Some(message) = extract_message(&value) else {
                    continue;
                };

                if require_meta_for_scan && !file_allowed {
                    // Try to infer project scope from early messages (e.g. <environment_context>
                    // <cwd>...</cwd>) when session_meta.cwd is generic like "/home/user".
                    if message.role == "user" || message.role == "developer" {
                        let hints = extract_path_hints_from_text(&message.text);
                        if !hints.is_empty() {
                            meta.path_hints.extend(hints);
                            meta.path_hints.sort();
                            meta.path_hints.dedup();
                            file_allowed = session_matches_cwd_prefix(&meta, &cwd_prefix);
                            if file_allowed && !project_counted {
                                let hint = infer_project_hint(&meta, &fallback_id_seed);
                                upsert_project_summary(&mut project_summary, &hint, true, false);
                                file_project = Some(hint);
                                project_counted = true;
                            }
                        }
                    }

                    if !file_allowed {
                        if scanned_bytes_in_file > hint_scan_limit_bytes {
                            break;
                        }
                        continue;
                    }
                }

                if !role_matches(&message, role.as_deref()) {
                    continue;
                }
                let Some(match_byte) = message.text.find(query) else {
                    continue;
                };
                if hits.len() >= hits_limit {
                    break;
                }

                let project_hint = file_project
                    .clone()
                    .unwrap_or_else(|| infer_project_hint(&meta, &fallback_id_seed));
                if file_project.is_none() {
                    file_project = Some(project_hint.clone());
                }

                if dedupe {
                    let normalized = normalize_snippet_whitespace(&message.text);
                    let msg_hash = stable_msg_hash(&normalized);
                    let key = format!("p:{}:r:{}:h:{msg_hash}", project_hint.id, message.role);
                    if !seen_hits.insert(key) {
                        continue;
                    }
                }
                let snippet =
                    slice_around_match(&message.text, match_byte, query.len(), context_chars);

                upsert_project_summary(&mut project_summary, &project_hint, false, true);
                hits.push(json!({
                    "ref": { "path": rel_path.clone(), "line": line_no },
                    "session": {
                        "id": meta.id,
                        "ts": meta.ts
                    },
                    "project": {
                        "id": project_hint.id,
                        "name": project_hint.name,
                        "confidence": project_hint.confidence
                    },
                    "message": {
                        "role": message.role,
                        "ts": message.ts,
                        "snippet": snippet
                    }
                }));
            }
        }

        let projects = render_project_summary(&project_summary, 20);
        let mut result = json!({
            "workspace": workspace.as_str(),
            "root_dir": root_dir_raw.clone(),
            "filters": {
                "cwd_prefix": cwd_prefix,
                "role": role
            },
            "scanned": {
                "files": scanned_files,
                "bytes": scanned_bytes,
                "truncated": scan_truncated
            },
            "projects": projects,
            "hits": hits,
            "truncated": false
        });

        redact_value(&mut result, 6);

        let mut suggestions = Vec::new();
        if let Some(first) = result
            .get("hits")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            && let (Some(path), Some(line)) = (
                first
                    .get("ref")
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str()),
                first
                    .get("ref")
                    .and_then(|v| v.get("line"))
                    .and_then(|v| v.as_u64()),
            )
        {
            suggestions.push(suggest_call(
                "transcripts_open",
                "Open the first matching transcript hit (bounded).",
                "low",
                json!({
                    "workspace": workspace.as_str(),
                    "root_dir": root_dir_raw.clone(),
                    "ref": { "path": path, "line": line }
                }),
            ));
        }

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= trim_array_to_budget(value, &["hits"], limit, false);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["scanned"], &["bytes"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["scanned"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["filters"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["projects"]);
                    }
                    if json_len_chars(value) > limit && retain_one_at(value, &["hits"], false) {
                        changed = true;
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok_with("transcripts_search", result, suggestions)
        } else {
            ai_ok_with_warnings("transcripts_search", result, warnings, suggestions)
        }
    }
}

fn role_matches(message: &ExtractedMessage, role: Option<&str>) -> bool {
    let Some(role) = role else {
        return true;
    };
    message.role == role
}

fn relativize_path(root_dir: &Path, file_path: &Path) -> String {
    file_path
        .strip_prefix(root_dir)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string_lossy().to_string())
}

fn stable_msg_hash(text: &str) -> String {
    // Stable, dependency-free hash: FNV-1a 64 over normalized text.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn upsert_project_summary(
    map: &mut BTreeMap<String, Value>,
    hint: &ProjectHint,
    add_file: bool,
    add_hit: bool,
) {
    let entry = map
        .entry(hint.id.clone())
        .or_insert_with(|| json!({ "id": hint.id, "name": hint.name, "confidence": hint.confidence, "files": 0, "hits": 0 }));
    if let Some(obj) = entry.as_object_mut() {
        if add_file {
            let files = obj.get("files").and_then(|v| v.as_u64()).unwrap_or(0);
            obj.insert("files".to_string(), json!(files + 1));
        }
        if add_hit {
            let hits = obj.get("hits").and_then(|v| v.as_u64()).unwrap_or(0);
            obj.insert("hits".to_string(), json!(hits + 1));
        }
    }
}

fn render_project_summary(map: &BTreeMap<String, Value>, limit: usize) -> Vec<Value> {
    let mut out = Vec::new();
    for (_id, item) in map.iter() {
        out.push(item.clone());
        if out.len() >= limit {
            break;
        }
    }
    out
}
