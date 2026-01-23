#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn core_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "init",
            "description": "Initialize workspace storage and bootstrap the default branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "status",
            "description": "Get reasoning store status for a workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "workspace_use",
            "description": "Switch the active workspace for this session (no restart).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "open",
            "description": "Open a single artifact by stable id/reference (CARD-..., <doc>@<seq>, a:<anchor>, runner:<id>, TASK-..., PLAN-..., JOB-...).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "id": { "type": "string" },
                    "limit": { "type": "integer" },
                    "include_drafts": { "type": "boolean" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "help",
            "description": "Agent-first help: protocol semantics, proof conventions, and the daily portal workflow.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "skill",
            "description": "Get a built-in behavior pack (daily|strict|research|teamlead) to shape agent workflow deterministically.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profile": {
                        "type": "string",
                        "enum": ["daily", "strict", "research", "teamlead"]
                    },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "diagnostics",
            "description": "Workspace diagnostics: what is broken and how to recover.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
