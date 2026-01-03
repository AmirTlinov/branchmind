#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn trace_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "trace_step",
            "description": "Append a structured trace step entry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "step": { "type": "string" },
                    "target": { "type": "string" },
                    "doc": { "type": "string" },
                    "message": { "type": "string" },
                    "mode": { "type": "string" },
                    "supports": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "blocks": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "base": { "type": "string" },
                    "checkpoint_every": { "type": "integer" },
                    "meta": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    }
                },
                "required": ["workspace", "step"]
            }
        }),
        json!({
            "name": "trace_sequential_step",
            "description": "Append a step in a sequential trace (with ordering metadata).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "doc": { "type": "string" },
                    "target": { "type": "string" },
                    "thought": { "type": "string" },
                    "thoughtNumber": { "type": "integer" },
                    "totalThoughts": { "type": "integer" },
                    "nextThoughtNeeded": { "type": "boolean" },
                    "isRevision": { "type": "boolean" },
                    "revisesThought": { "type": "integer" },
                    "branchFromThought": { "type": "integer" },
                    "branchId": { "type": "string" },
                    "needsMoreThoughts": { "type": "string" },
                    "confidence": { "type": "string" },
                    "goal": { "type": "string" },
                    "message": { "type": "string" },
                    "meta": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    }
                },
                "required": ["workspace", "thought", "thoughtNumber", "totalThoughts", "nextThoughtNeeded"]
            }
        }),
        json!({
            "name": "trace_hydrate",
            "description": "Return a bounded trace slice for fast resumption.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "doc": { "type": "string" },
                    "limit_steps": { "type": "integer" },
                    "statement_max_bytes": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "trace_validate",
            "description": "Validate trace invariants (ordering, required fields).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "doc": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
