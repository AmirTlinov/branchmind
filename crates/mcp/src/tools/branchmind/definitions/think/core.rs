#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think_template",
            "description": "Return a deterministic thinking card skeleton for a supported type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "type": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "type"]
            }
        }),
        json!({
            "name": "think_card",
            "description": "Atomically commit a thinking card into trace_doc and upsert node/edges into graph_doc. Defaults to checkout+docs and auto-id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_pipeline",
            "description": "Canonical pipeline: frame → hypothesis → test → evidence → decision (auto-link + optional decision note).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "notes_doc": { "type": "string" },
                    "frame": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "hypothesis": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "test": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "evidence": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "decision": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "status": { "type": "object" },
                    "note_decision": { "type": "boolean" },
                    "note_title": { "type": "string" },
                    "note_format": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
