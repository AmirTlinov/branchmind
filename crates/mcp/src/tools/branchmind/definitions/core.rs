#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn core_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "init",
            "description": "Initialize workspace storage and bootstrap the default branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "status",
            "description": "Get reasoning store status for a workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "help",
            "description": "Agent-first help: protocol semantics, proof conventions, and the daily portal workflow.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "diagnostics",
            "description": "Workspace diagnostics: what is broken and how to recover.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
