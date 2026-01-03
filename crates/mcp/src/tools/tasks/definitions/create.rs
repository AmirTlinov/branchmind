#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn create_definitions() -> Vec<Value> {
    vec![json!({
        "name": "tasks_create",
        "description": "Create a plan or a task (v0 skeleton).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "kind": { "type": "string", "enum": ["plan", "task"] },
                "parent": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string" },
                "contract": { "type": "string" },
                "contract_data": { "type": "object" },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "success_criteria": { "type": "array", "items": { "type": "string" } },
                            "tests": { "type": "array", "items": { "type": "string" } },
                            "blockers": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["title", "success_criteria"]
                    }
                }
            },
            "required": ["workspace", "title"]
        }
    })]
}
