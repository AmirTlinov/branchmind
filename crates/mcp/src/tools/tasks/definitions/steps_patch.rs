#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn steps_patch_definitions() -> Vec<Value> {
    vec![json!({
        "name": "tasks_patch",
        "description": "Diff-oriented updates for task detail, step, or task node.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "task": { "type": "string" },
                "expected_revision": { "type": "integer" },
                "kind": { "type": "string", "enum": ["task_detail", "step", "task"] },
                "path": { "type": "string" },
                "step_id": { "type": "string" },
                "task_node_id": { "type": "string" },
                "ops": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "op": { "type": "string", "enum": ["set", "unset", "append", "remove"] },
                            "field": { "type": "string" },
                            "value": {}
                        },
                        "required": ["op", "field"]
                    }
                }
            },
            "required": ["workspace", "ops"]
        }
    })]
}
