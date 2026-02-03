#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![json!({
        "name": "think_playbook",
        "description": "Return a deterministic playbook skeleton by name.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "name": { "type": "string" },
                "max_chars": { "type": "integer" }
            },
            "required": ["workspace", "name"]
        }
    })]
}
