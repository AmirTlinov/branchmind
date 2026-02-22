#![forbid(unsafe_code)]

use serde_json::{Value, json};

fn markdown_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "workspace": { "type": "string" },
            "markdown": { "type": "string" }
        },
        "required": ["workspace", "markdown"]
    })
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think",
            "description": "Thought commits and history from markdown bm command blocks.",
            "inputSchema": markdown_schema(),
        }),
        json!({
            "name": "branch",
            "description": "Branch operations from markdown bm command blocks.",
            "inputSchema": markdown_schema(),
        }),
        json!({
            "name": "merge",
            "description": "Branch merge synthesis from markdown bm command blocks.",
            "inputSchema": markdown_schema(),
        }),
    ]
}
