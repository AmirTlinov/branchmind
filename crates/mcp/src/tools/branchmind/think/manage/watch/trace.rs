#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct TraceTail {
    pub(super) entries: Vec<Value>,
    pub(super) next_cursor: Option<i64>,
    pub(super) has_more: bool,
    pub(super) count: usize,
}

pub(super) fn fetch(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    trace_doc: &str,
    trace_limit_steps: usize,
    trace_statement_max_bytes: Option<usize>,
) -> Result<TraceTail, Value> {
    let trace_slice =
        match server
            .store
            .doc_show_tail(workspace, branch, trace_doc, None, trace_limit_steps)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

    let mut entries = doc_entries_to_json(trace_slice.entries);
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
        next_cursor: trace_slice.next_cursor,
        has_more: trace_slice.has_more,
        count,
    })
}
