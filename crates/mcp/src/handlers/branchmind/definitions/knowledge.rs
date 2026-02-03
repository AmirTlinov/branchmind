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
                "key": { "type": "string", "description": "Knowledge key slug (adds tags_all k:<slug>)."},
                "agent_id": { "type": "string" },
                "include_drafts": { "type": "boolean", "description": "Include draft-lane knowledge (default true). Alias for all_lanes." },
                "include_history": { "type": "boolean", "description": "When true, return all versions; when false (default), return latest-only (deduped)." },
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
