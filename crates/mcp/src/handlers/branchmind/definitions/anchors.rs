#![forbid(unsafe_code)]

use serde_json::{Value, json};

fn think_card_type_enum() -> Vec<Value> {
    bm_core::think::SUPPORTED_THINK_CARD_TYPES
        .iter()
        .map(|value| Value::String(value.to_string()))
        .collect()
}

pub(crate) fn anchors_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "anchors_list",
            "description": "List known architecture anchors (meaning map).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "text": { "type": "string" },
                    "kind": { "type": "string" },
                    "status": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "anchor_snapshot",
            "description": "Show a bounded, low-noise snapshot for a single anchor (meaning-scoped context).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "anchor": { "type": "string" },
                    "include_drafts": { "type": "boolean" },
                    "tasks_limit": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "anchor"]
            }
        }),
        json!({
            "name": "macro_anchor_note",
            "description": "One-command flow: upsert anchor metadata + write an anchor-tagged card (optionally step/target scoped).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "anchor": { "type": "string" },
                    "title": { "type": "string" },
                    "kind": { "type": "string" },
                    "status": { "type": "string" },
                    "description": { "type": ["string", "null"] },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "aliases": { "type": "array", "items": { "type": "string" } },
                    "parent_id": { "type": ["string", "null"] },
                    "depends_on": { "type": "array", "items": { "type": "string" } },
                    "content": { "type": "string" },
                    "card_type": { "type": "string", "enum": think_card_type_enum() },
                    "step": { "type": "string" },
                    "visibility": { "type": "string", "enum": ["draft", "canon"] },
                    "pin": { "type": "boolean" }
                },
                "required": ["workspace", "anchor", "content"]
            }
        }),
        json!({
            "name": "anchors_export",
            "description": "Export the anchor map as deterministic text (mermaid or adjacency list).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "format": { "type": "string", "enum": ["mermaid", "text"] },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "anchors_rename",
            "description": "Rename an anchor id without retagging history (refactor-grade).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" }
                },
                "required": ["workspace", "from", "to"]
            }
        }),
        json!({
            "name": "anchors_bootstrap",
            "description": "Seed the meaning map: create/update multiple anchors deterministically in one call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "anchors": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "kind": { "type": "string" },
                                "status": { "type": "string" },
                                "description": { "type": ["string", "null"] },
                                "refs": { "type": "array", "items": { "type": "string" } },
                                "aliases": { "type": "array", "items": { "type": "string" } },
                                "parent_id": { "type": ["string", "null"] },
                                "depends_on": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["id", "title", "kind"]
                        }
                    },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "anchors"]
            }
        }),
        json!({
            "name": "anchors_merge",
            "description": "Merge anchors into a canonical id (scale hygiene): keep history via alias mapping, avoid taxonomy explosion.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "into": { "type": "string" },
                    "from": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "into", "from"]
            }
        }),
        json!({
            "name": "anchors_lint",
            "description": "Bounded health check for the meaning map (orphans, unknown deps, alias drift).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
