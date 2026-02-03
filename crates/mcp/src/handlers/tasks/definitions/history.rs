#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn history_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_history",
            "description": "Get operation history (undo/redo metadata).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_undo",
            "description": "Undo the most recent undoable operation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_redo",
            "description": "Redo the most recent undone operation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
