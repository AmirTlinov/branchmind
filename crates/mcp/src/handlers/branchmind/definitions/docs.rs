#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn docs_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "show",
            "description": "Read a bounded slice (tail/pagination) of a reasoning document. Defaults to checkout+doc_kind.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "doc_kind": { "type": "string", "enum": ["notes", "trace", "plan_spec"] },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "diff",
            "description": "Directional diff between two branches for a single document.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "doc_kind": { "type": "string", "enum": ["notes", "trace", "plan_spec"] },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "from", "to"]
            }
        }),
        json!({
            "name": "merge",
            "description": "Idempotent merge of note entries from one branch into another (notes VCS).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc_kind": { "type": "string", "enum": ["notes", "trace", "plan_spec"] },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "from", "into"]
            }
        }),
    ]
}
