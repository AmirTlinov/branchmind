#![forbid(unsafe_code)]

use super::json::parse_json_or_string;
use super::time::ts_ms_to_rfc3339;
use bm_storage::{DocEntryKind, DocEntryRow};
use serde_json::{Value, json};

fn lift_step_meta(meta: &mut Value) {
    let Some(obj) = meta.as_object_mut() else {
        return;
    };
    if obj.get("step").is_some() {
        return;
    }
    let Some(step) = obj.get("meta").and_then(|v| v.get("step")).cloned() else {
        return;
    };
    obj.insert("step".to_string(), step);
}

fn doc_entry_to_json(entry: DocEntryRow) -> Value {
    match entry.kind {
        DocEntryKind::Note => {
            let mut meta = entry
                .meta_json
                .as_ref()
                .map(|raw| parse_json_or_string(raw))
                .unwrap_or(Value::Null);
            // Normalize step scoping for callers: accept both legacy `meta.step` and wrapped
            // `meta.meta.step`, but prefer a single presentation surface.
            lift_step_meta(&mut meta);

            json!({
                "seq": entry.seq,
                "ts": ts_ms_to_rfc3339(entry.ts_ms),
                "ts_ms": entry.ts_ms,
                "kind": entry.kind.as_str(),
                "title": entry.title,
                "format": entry.format,
                "meta": meta,
                "content": entry.content
            })
        }
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
