#![forbid(unsafe_code)]

use super::json::parse_json_or_string;
use super::time::ts_ms_to_rfc3339;
use bm_storage::EventRow;
use serde_json::{Value, json};

pub(crate) fn sort_events_by_seq(events: &mut [EventRow]) {
    events.sort_by(|a, b| {
        a.seq
            .cmp(&b.seq)
            .then_with(|| a.event_type.cmp(&b.event_type))
    });
}

pub(crate) fn events_to_json(events: Vec<EventRow>) -> Vec<Value> {
    events
        .into_iter()
        .map(|event| {
            json!({
                "event_id": event.event_id(),
                "ts": ts_ms_to_rfc3339(event.ts_ms),
                "ts_ms": event.ts_ms,
                "task_id": event.task_id,
                "path": event.path,
                "type": event.event_type,
                "payload": parse_json_or_string(&event.payload_json)
            })
        })
        .collect()
}
