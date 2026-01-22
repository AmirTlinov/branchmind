#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct TraceTail {
    pub(super) entries: Vec<Value>,
    pub(super) next_cursor: Option<i64>,
    pub(super) has_more: bool,
    pub(super) count: usize,
}

pub(super) struct TraceFetchRequest<'a> {
    pub(super) branch: &'a str,
    pub(super) trace_doc: &'a str,
    pub(super) trace_limit_steps: usize,
    pub(super) trace_statement_max_bytes: Option<usize>,
    pub(super) agent_id: Option<&'a str>,
    pub(super) all_lanes: bool,
    pub(super) focus_task_id: Option<&'a str>,
    pub(super) focus_step_path: Option<&'a str>,
}

pub(super) fn fetch(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    request: TraceFetchRequest<'_>,
) -> Result<TraceTail, Value> {
    let TraceFetchRequest {
        branch,
        trace_doc,
        trace_limit_steps,
        trace_statement_max_bytes,
        agent_id,
        all_lanes,
        focus_task_id,
        focus_step_path,
    } = request;

    if trace_limit_steps == 0 {
        return Ok(TraceTail {
            entries: Vec::new(),
            next_cursor: None,
            has_more: false,
            count: 0,
        });
    }

    let want_limit = trace_limit_steps.min(200);

    let keep_step = focus_task_id.zip(focus_step_path);
    if keep_step.is_none() {
        let trace_slice = match server
            .store
            .doc_show_tail(workspace, branch, trace_doc, None, want_limit)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        let mut entries = doc_entries_to_json(trace_slice.entries);
        if !all_lanes {
            entries.retain(|entry| {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    return true;
                }
                let meta = entry.get("meta").unwrap_or(&Value::Null);
                lane_matches_meta(meta, agent_id)
            });
        }

        if let Some(max_bytes) = trace_statement_max_bytes {
            for entry in &mut entries {
                let Some(content) = entry.get("content").and_then(|v| v.as_str()) else {
                    continue;
                };
                let trimmed = truncate_string_bytes(content, max_bytes);
                let Some(obj) = entry.as_object_mut() else {
                    continue;
                };
                obj.insert("content".to_string(), Value::String(trimmed));
            }
        }

        return Ok(TraceTail {
            count: entries.len(),
            entries,
            next_cursor: trace_slice.next_cursor,
            has_more: trace_slice.has_more,
        });
    }

    let (focus_task_id, focus_step_path) = keep_step.expect("guarded above");

    let mut cursor: Option<i64> = None;
    let mut out_desc = Vec::<Value>::new();
    let mut last_next_cursor = None;
    let mut last_has_more = false;

    // Deterministic bounded scan: collect only entries scoped to the focus step.
    for _ in 0..3 {
        let slice = match server
            .store
            .doc_show_tail(workspace, branch, trace_doc, cursor, 200)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        last_next_cursor = slice.next_cursor;
        last_has_more = slice.has_more;

        let entries = doc_entries_to_json(slice.entries);
        for entry in entries.iter().rev() {
            let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");

            let keep = match kind {
                "event" => {
                    let entry_task = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                    if entry_task != focus_task_id {
                        false
                    } else if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                        step_path_matches(focus_step_path, path)
                    } else {
                        false
                    }
                }
                "note" => {
                    let meta = entry.get("meta").unwrap_or(&Value::Null);
                    if !step_meta_matches(meta, focus_task_id, focus_step_path) {
                        false
                    } else if all_lanes {
                        true
                    } else {
                        lane_matches_meta(meta, agent_id)
                    }
                }
                _ => false,
            };

            if keep {
                out_desc.push(entry.clone());
                if out_desc.len() >= want_limit {
                    break;
                }
            }
        }

        if out_desc.len() >= want_limit {
            break;
        }
        if !last_has_more {
            break;
        }
        cursor = last_next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let mut has_more = last_has_more;
    if out_desc.len() >= want_limit {
        has_more = true;
    }
    out_desc.truncate(want_limit);
    out_desc.reverse();

    let mut entries = out_desc;
    let count = entries.len();

    if let Some(max_bytes) = trace_statement_max_bytes {
        for entry in &mut entries {
            let Some(content) = entry.get("content").and_then(|v| v.as_str()) else {
                continue;
            };
            let trimmed = truncate_string_bytes(content, max_bytes);
            let Some(obj) = entry.as_object_mut() else {
                continue;
            };
            obj.insert("content".to_string(), Value::String(trimmed));
        }
    }

    Ok(TraceTail {
        entries,
        next_cursor: last_next_cursor,
        has_more,
        count,
    })
}
