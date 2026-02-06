#![forbid(unsafe_code)]

use crate::Toolset;
use crate::ops::{CommandRegistry, Tier, ToolName};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn tier_allowed(toolset: Toolset, tier: Tier) -> bool {
    tier.allowed_in_toolset(toolset)
}

fn collect_op_aliases(tool: ToolName, toolset: Toolset) -> Vec<String> {
    let registry = CommandRegistry::global();
    let mut out = BTreeSet::<String>::new();

    for spec in registry.specs() {
        if spec.domain_tool != tool {
            continue;
        }
        if !tier_allowed(toolset, spec.tier) {
            continue;
        }
        for alias in spec.op_aliases.iter() {
            let trimmed = alias.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.insert(trimmed.to_string());
        }
    }

    out.into_iter().collect::<Vec<_>>()
}

fn ops_schema(tool: ToolName, toolset: Toolset) -> Value {
    let mut ops = collect_op_aliases(tool, toolset)
        .into_iter()
        .map(|s| json!(s))
        .collect::<Vec<_>>();
    ops.push(json!("call"));
    json!({
        "type": "object",
        "properties": {
            "workspace": { "type": "string" },
            "op": { "type": "string", "enum": ops },
            "cmd": { "type": "string" },
            "args": { "type": "object" },
            "budget_profile": { "type": "string", "enum": ["portal", "default", "audit"] },
            "view": { "type": "string", "enum": ["compact", "smart", "audit"] }
        },
        "required": ["op", "args"]
    })
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    tool_definitions_for(Toolset::Full)
}

pub(crate) fn tool_definitions_for(toolset: Toolset) -> Vec<Value> {
    vec![
        json!({
            "name": "status",
            "description": "Workspace status + NextEngine actions (v1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "budget_profile": { "type": "string", "enum": ["portal", "default", "audit"] },
                    "view": { "type": "string", "enum": ["compact", "smart", "audit"] }
                },
                "required": []
            }
        }),
        json!({
            "name": "open",
            "description": "Open an artifact by id/ref (v1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "id": { "type": "string" },
                    "limit": { "type": "integer" },
                    "include_drafts": { "type": "boolean" },
                    "include_content": { "type": "boolean" },
                    "budget_profile": { "type": "string", "enum": ["portal", "default", "audit"] },
                    "view": { "type": "string", "enum": ["compact", "smart", "audit"] }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "workspace",
            "description": "Workspace operations (v1).",
            "inputSchema": ops_schema(ToolName::WorkspaceOps, toolset)
        }),
        json!({
            "name": "tasks",
            "description": "Tasks operations (v1).",
            "inputSchema": ops_schema(ToolName::TasksOps, toolset)
        }),
        json!({
            "name": "jobs",
            "description": "Delegation jobs operations (v1).",
            "inputSchema": ops_schema(ToolName::JobsOps, toolset)
        }),
        json!({
            "name": "think",
            "description": "Reasoning/knowledge operations (v1).",
            "inputSchema": ops_schema(ToolName::ThinkOps, toolset)
        }),
        json!({
            "name": "graph",
            "description": "Graph operations (v1).",
            "inputSchema": ops_schema(ToolName::GraphOps, toolset)
        }),
        json!({
            "name": "vcs",
            "description": "VCS operations (v1).",
            "inputSchema": ops_schema(ToolName::VcsOps, toolset)
        }),
        json!({
            "name": "docs",
            "description": "Docs operations (v1).",
            "inputSchema": ops_schema(ToolName::DocsOps, toolset)
        }),
        json!({
            "name": "system",
            "description": "System operations (v1).",
            "inputSchema": ops_schema(ToolName::SystemOps, toolset)
        }),
    ]
}
