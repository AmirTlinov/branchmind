#![forbid(unsafe_code)]

use super::json::parse_json_or_string;
use super::time::ts_ms_to_rfc3339;
use bm_storage::{DocEntryKind, DocEntryRow};
use serde_json::{Value, json};

fn doc_entry_to_json(entry: DocEntryRow) -> Value {
    match entry.kind {
        DocEntryKind::Note => json!({
            "seq": entry.seq,
            "ts": ts_ms_to_rfc3339(entry.ts_ms),
            "ts_ms": entry.ts_ms,
            "kind": entry.kind.as_str(),
            "title": entry.title,
            "format": entry.format,
            "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
            "content": entry.content
        }),
        DocEntryKind::Event => json!({
            "seq": entry.seq,
            "ts": ts_ms_to_rfc3339(entry.ts_ms),
            "ts_ms": entry.ts_ms,
            "kind": entry.kind.as_str(),
            "event_id": entry.source_event_id,
            "event_type": entry.event_type,
            "task_id": entry.task_id,
            "path": entry.path
        }),
    }
}

pub(crate) fn doc_entries_to_json(entries: Vec<DocEntryRow>) -> Vec<Value> {
    entries.into_iter().map(doc_entry_to_json).collect()
}
