#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn notes_vcs_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "notes_commit",
            "description": "Append a note entry to the notes document of a target or an explicit (branch, doc). Defaults to checkout+notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "content": { "type": "string" },
                    "title": { "type": "string" },
                    "format": { "type": "string" },
                    "meta": { "type": "object" },
                    "agent_id": { "type": "string" },
                    "promote_to_knowledge": { "type": "boolean" },
                    "knowledge_anchor": { "type": "string" },
                    "knowledge_key": { "type": "string" },
                    "knowledge_title": { "type": "string" },
                    "knowledge_key_mode": { "type": "string", "enum": ["explicit", "auto"] }
                },
                "required": ["workspace", "content"]
            }
        }),
        json!({
            "name": "commit",
            "description": "Append a VCS-style notes commit (artifact + message) to one or more docs. Defaults to checkout+notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "artifact": { "type": "string" },
                    "message": { "type": "string" },
                    "docs": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "artifact", "message"]
            }
        }),
        json!({
            "name": "log",
            "description": "Return recent commit-like entries for a branch/ref.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "docs_list",
            "description": "List known documents for a branch/ref.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tag_create",
            "description": "Create or update a lightweight tag pointing to a commit entry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" },
                    "from": { "type": "string" },
                    "force": { "type": "boolean" }
                },
                "required": ["workspace", "name"]
            }
        }),
        json!({
            "name": "tag_list",
            "description": "List lightweight tags for a workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tag_delete",
            "description": "Delete a lightweight tag.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["workspace", "name"]
            }
        }),
        json!({
            "name": "reflog",
            "description": "Return ref movements for the VCS-style surface.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "reset",
            "description": "Move the current branch ref pointer to a specified commit entry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" }
                },
                "required": ["workspace", "ref"]
            }
        }),
    ]
}
