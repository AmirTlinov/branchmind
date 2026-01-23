#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn branches_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "branch_create",
            "description": "Create a new branch ref from an existing branch snapshot (no copy). Defaults to checkout when from is omitted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" },
                    "from": { "type": "string" }
                },
                "required": ["workspace", "name"]
            }
        }),
        json!({
            "name": "macro_branch_note",
            "description": "One-call: (optional) branch checkout/create by name + append a note (daily portal).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" },
                    "from": { "type": "string" },
                    "doc": { "type": "string" },
                    "content": { "type": "string" },
                    "template": { "type": "string" },
                    "goal": { "type": "string" },
                    "title": { "type": "string" },
                    "format": { "type": "string" },
                    "meta": { "type": "object" },
                    "agent_id": { "type": "string" }
                },
                "required": []
            }
        }),
        json!({
            "name": "branch_list",
            "description": "List known branch refs for a workspace (including canonical task/plan branches).",
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
        json!({
            "name": "checkout",
            "description": "Set the current workspace branch ref (does not affect tasks).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" }
                },
                "required": ["workspace", "ref"]
            }
        }),
        json!({
            "name": "branch_rename",
            "description": "Rename an existing branch ref and update dependent artifacts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "old": { "type": "string" },
                    "new": { "type": "string" }
                },
                "required": ["workspace", "old", "new"]
            }
        }),
        json!({
            "name": "branch_delete",
            "description": "Delete a branch ref and its data if safe.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["workspace", "name"]
            }
        }),
    ]
}
