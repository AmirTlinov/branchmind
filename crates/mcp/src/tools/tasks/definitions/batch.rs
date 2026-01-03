#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn batch_definitions() -> Vec<Value> {
    vec![json!({
        "name": "tasks_batch",
        "description": "Run multiple task operations atomically.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "atomic": { "type": "boolean" },
                "compact": { "type": "boolean" },
                "operations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "name": { "type": "string" },
                            "args": { "type": "object" },
                            "arguments": { "type": "object" }
                        }
                    }
                }
            },
            "required": ["workspace", "operations"]
        }
    })]
}
