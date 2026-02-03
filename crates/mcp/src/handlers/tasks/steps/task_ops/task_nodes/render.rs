#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn task_node_op_result_json(
    task_id: String,
    out: bm_storage::TaskNodeOpResult,
) -> Value {
    json!({
        "task": task_id,
        "revision": out.task_revision,
        "node": { "node_id": out.node.node_id, "path": out.node.path },
        "event": {
            "event_id": out.event.event_id(),
            "ts": ts_ms_to_rfc3339(out.event.ts_ms),
            "ts_ms": out.event.ts_ms,
            "task_id": out.event.task_id,
            "path": out.event.path,
            "type": out.event.event_type,
            "payload": parse_json_or_string(&out.event.payload_json)
        }
    })
}
