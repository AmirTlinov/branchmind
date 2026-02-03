#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn steps_control_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_block",
            "description": "Block/unblock a step path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "blocked": { "type": "boolean" },
                    "reason": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_progress",
            "description": "Mark a step path completed/uncompleted (respects checkpoints unless force=true).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "completed": { "type": "boolean" },
                    "force": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
