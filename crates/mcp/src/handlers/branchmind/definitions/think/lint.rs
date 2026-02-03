#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![json!({
        "name": "think_lint",
        "description": "Validate think graph invariants and report issues.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "target": { "type": "string" },
                "ref": { "type": "string" },
                "graph_doc": { "type": "string" },
                "max_chars": { "type": "integer" }
            },
            "required": ["workspace"]
        }
    })]
}
