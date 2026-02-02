#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think_add_hypothesis",
            "description": "Create a hypothesis card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_question",
            "description": "Create a question card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_test",
            "description": "Create a test card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_note",
            "description": "Create a note card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_decision",
            "description": "Create a decision card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_evidence",
            "description": "Create an evidence card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_knowledge",
            "description": "Create a knowledge card (wrapper over think_card with v:canon default).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "anchor": { "type": "string", "description": "Anchor slug or a:<slug> (adds tag)." },
                    "key": { "type": "string", "description": "Knowledge key slug (adds k:<slug> tag)." },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_frame",
            "description": "Create a frame card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "think_add_update",
            "description": "Create an update card (wrapper over think_card).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "verbosity": { "type": "string", "enum": ["full", "compact"] }
                },
                "required": ["workspace", "card"]
            }
        }),
    ]
}
