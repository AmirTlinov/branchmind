#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn knowledge_definitions() -> Vec<Value> {
    vec![json!({
        "name": "knowledge_list",
        "description": "List knowledge cards (type=knowledge), optionally filtered by anchor/tags.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "target": { "type": "string" },
                "ref": { "type": "string" },
                "graph_doc": { "type": "string" },
                "anchor": { "type": "string", "description": "Anchor slug or a:<slug> (adds tags_all a:<slug>)."},
                "agent_id": { "type": "string" },
                "include_drafts": { "type": "boolean", "description": "Alias for all_lanes (disable draft filtering)." },
                "all_lanes": { "type": "boolean" },
                "ids": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                },
                "status": { "type": "string" },
                "tags_any": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                },
                "tags_all": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                },
                "text": { "type": "string" },
                "limit": { "type": "integer" },
                "context_budget": { "type": "integer" },
                "max_chars": { "type": "integer" }
            },
            "required": ["workspace"]
        }
    })]
}
