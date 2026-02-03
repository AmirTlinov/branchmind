#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think_link",
            "description": "Create a graph edge between cards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "rel": { "type": "string" },
                    "to": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "message": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "from", "rel", "to"]
            }
        }),
        json!({
            "name": "think_set_status",
            "description": "Set status for one or more card ids.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "status": { "type": "string" },
                    "targets": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "message": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "status", "targets"]
            }
        }),
        json!({
            "name": "think_pin",
            "description": "Pin or unpin cards (tags-based).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "targets": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "pinned": { "type": "boolean" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" }
                },
                "required": ["workspace", "targets"]
            }
        }),
        json!({
            "name": "think_pins",
            "description": "List pinned cards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "think_publish",
            "description": "Promote a card into the shared lane (deterministic published copy).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "card_id": { "type": "string" },
                    "pin": { "type": "boolean" },
                    "agent_id": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "trace_doc": { "type": "string" }
                },
                "required": ["workspace", "card_id"]
            }
        }),
        json!({
            "name": "think_nominal_merge",
            "description": "Deduplicate similar cards into a canonical one.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "candidate_ids": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "limit_candidates": { "type": "integer" },
                    "limit_groups": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
