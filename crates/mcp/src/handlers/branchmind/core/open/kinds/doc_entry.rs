#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_doc_entry_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    ref_str: &str,
    doc: String,
    seq: i64,
) -> Result<Value, Value> {
    let entry = match server.store.doc_entry_get_by_seq(workspace, seq) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(entry) = entry else {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown doc entry ref",
            Some(
                "Copy a <doc>@<seq> ref from snapshot delta or a prior notes_commit/think_* response.",
            ),
            vec![],
        ));
    };
    if entry.doc != doc {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "Doc prefix mismatch for ref",
            Some(&format!("Expected {}@{}", entry.doc, entry.seq)),
            vec![],
        ));
    }

    Ok(json!({
        "workspace": workspace.as_str(),
        "kind": "doc_entry",
        "ref": ref_str,
        "entry": {
            "seq": entry.seq,
            "ts": ts_ms_to_rfc3339(entry.ts_ms),
            "ts_ms": entry.ts_ms,
            "branch": entry.branch,
            "doc": entry.doc,
            "kind": entry.kind.as_str(),
            "title": entry.title,
            "format": entry.format,
            "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
            "content": entry.content,
            "payload": entry.payload_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
        },
        "summary": super::super::util::summary_one_line(
            entry.content.as_deref(),
            entry.title.as_deref(),
            120
        ),
        "truncated": false
    }))
}
