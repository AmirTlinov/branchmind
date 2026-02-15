#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName,
};

use serde_json::json;

use super::handlers;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // system.schema.get (custom)
    specs.push(CommandSpec {
        cmd: "system.schema.get".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.schema.get".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": { "cmd": { "type": "string" } },
                "required": ["cmd"]
            }),
            example_minimal_args: json!({ "cmd": "tasks.snapshot" }),
        },
        op_aliases: vec!["schema.get".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_schema_get),
    });

    // system.schema.list (custom)
    specs.push(CommandSpec {
        cmd: "system.schema.list".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.schema.list".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "portal": { "type": "string" },
                    "prefix": { "type": "string" },
                    "q": { "type": "string" },
                    "mode": {
                        "type": "string",
                        "enum": ["golden", "all", "names", "compact"]
                    },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({ "portal": "tasks", "q": "snapshot", "limit": 20 }),
        },
        op_aliases: vec!["schema.list".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_schema_list),
    });

    // system.cmd.list (custom)
    specs.push(CommandSpec {
        cmd: "system.cmd.list".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Advanced,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.cmd.list".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "prefix": { "type": "string" },
                    "q": { "type": "string" },
                    "mode": { "type": "string", "enum": ["golden", "all", "names", "compact"] },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                },
                "required": [],
                "additionalProperties": false
            }),
            example_minimal_args: json!({ "prefix": "tasks." }),
        },
        op_aliases: vec!["cmd.list".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_cmd_list),
    });

    // system.tools.list (custom)
    specs.push(CommandSpec {
        cmd: "system.tools.list".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.tools.list".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["tools.list".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_tools_list),
    });

    // system.quickstart (custom)
    specs.push(CommandSpec {
        cmd: "system.quickstart".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.quickstart".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "portal": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["portal"]
            }),
            example_minimal_args: json!({ "portal": "tasks" }),
        },
        op_aliases: vec!["quickstart".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_quickstart),
    });

    // system.exec.summary (custom)
    specs.push(CommandSpec {
        cmd: "system.exec.summary".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.exec.summary".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "include_tasks": { "type": "boolean" },
                    "include_jobs": { "type": "boolean" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "target": { "type": "string" },
                    "anchor": { "type": "string" },
                    "jobs_view": { "type": "string", "enum": ["smart", "audit"] },
                    "jobs_limit": { "type": "integer" },
                    "stall_after_s": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["exec.summary".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_exec_summary),
    });

    // system.ops.summary (custom)
    specs.push(CommandSpec {
        cmd: "system.ops.summary".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.ops.summary".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["ops.summary".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_ops_summary),
    });

    // system.migration.lookup (custom)
    specs.push(CommandSpec {
        cmd: "system.migration.lookup".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.migration.lookup".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "old_name": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["old_name"]
            }),
            example_minimal_args: json!({ "old_name": "tasks_snapshot" }),
        },
        op_aliases: vec!["migration.lookup".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_migration_lookup),
    });

    // system.tutorial (custom)
    specs.push(CommandSpec {
        cmd: "system.tutorial".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.tutorial".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["tutorial".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_tutorial),
    });

    // Minimal system tools exposed via cmd=system.<name>.
    for handler_name in ["storage", "init", "help", "skill", "diagnostics"] {
        let tier = match handler_name {
            "storage" | "diagnostics" => Tier::Internal,
            _ => Tier::Advanced,
        };
        specs.push(CommandSpec {
            cmd: format!("system.{handler_name}"),
            domain_tool: ToolName::SystemOps,
            tier,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: false,
                confirm_level: ConfirmLevel::None,
                idempotent: true,
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases: Vec::new(),
            handler_name: Some(handler_name.to_string()),
            handler: None,
        });
    }
}
