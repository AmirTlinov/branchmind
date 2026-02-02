#![forbid(unsafe_code)]

use serde_json::{Value, json};

fn ops_schema(golden_ops: &[&str]) -> Value {
    let mut ops = golden_ops.iter().map(|s| json!(s)).collect::<Vec<_>>();
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
            "inputSchema": ops_schema(&["use", "reset"])
        }),
        json!({
            "name": "tasks",
            "description": "Tasks operations (v1).",
            "inputSchema": ops_schema(&["plan.create", "plan.decompose", "execute.next", "evidence.capture", "step.close"])
        }),
        json!({
            "name": "jobs",
            "description": "Delegation jobs operations (v1).",
            "inputSchema": ops_schema(&["create", "list", "radar", "open", "runner.start"])
        }),
        json!({
            "name": "think",
            "description": "Reasoning/knowledge operations (v1).",
            "inputSchema": ops_schema(&[
                "knowledge.upsert",
                "knowledge.query",
                "knowledge.recall",
                "knowledge.lint",
                "reasoning.seed",
                "reasoning.pipeline",
                "idea.branch.create",
                "idea.branch.merge"
            ])
        }),
        json!({
            "name": "graph",
            "description": "Graph operations (v1).",
            "inputSchema": ops_schema(&["query", "apply", "merge"])
        }),
        json!({
            "name": "vcs",
            "description": "VCS operations (v1).",
            "inputSchema": ops_schema(&["branch.create"])
        }),
        json!({
            "name": "docs",
            "description": "Docs operations (v1).",
            "inputSchema": ops_schema(&["list", "show", "diff", "merge"])
        }),
        json!({
            "name": "system",
            "description": "System operations (v1).",
            "inputSchema": ops_schema(&["schema.get", "ops.summary", "cmd.list", "migration.lookup"])
        }),
    ]
}
