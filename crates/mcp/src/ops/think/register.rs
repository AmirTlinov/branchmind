#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName,
};

use serde_json::json;

use super::handlers;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    specs.push(CommandSpec {
        cmd: "think.reasoning.seed".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.reasoning.seed".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec!["reasoning.seed".to_string()],
        handler_name: Some("think_template".to_string()),
        handler: Some(handlers::handle_reasoning_seed),
    });

    specs.push(CommandSpec {
        cmd: "think.reasoning.pipeline".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.reasoning.pipeline".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec!["reasoning.pipeline".to_string()],
        handler_name: Some("think_pipeline".to_string()),
        handler: Some(handlers::handle_reasoning_pipeline),
    });

    // Idea-branch helpers as golden ops (handler-backed for v1).
    specs.push(CommandSpec {
        cmd: "think.idea.branch.create".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.idea.branch.create".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec!["idea.branch.create".to_string()],
        handler_name: Some("macro_branch_note".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "think.idea.branch.merge".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.idea.branch.merge".to_string(),
        },
        safety: Safety {
            destructive: true,
            confirm_level: ConfirmLevel::Soft,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["from", "into"]
            }),
            example_minimal_args: json!({
                "from": "<idea-branch>",
                "into": "<target-branch>",
                "dry_run": true
            }),
        },
        op_aliases: vec!["idea.branch.merge".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_idea_branch_merge),
    });

    // Atlas helpers (directory → anchor bindings) as first-class v1 cmds.
    specs.push(CommandSpec {
        cmd: "think.atlas.suggest".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.atlas.suggest".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec![],
        handler_name: Some("atlas_suggest".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "think.macro.atlas.apply".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.macro.atlas.apply".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec![],
        handler_name: Some("macro_atlas_apply".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "think.atlas.bindings.list".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.atlas.bindings.list".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec![],
        handler_name: Some("atlas_bindings_list".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "think.macro.counter.hypothesis.stub".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.macro.counter.hypothesis.stub".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            // When card ids are omitted, the macro creates new cards each call.
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec![],
        handler_name: Some("think_macro_counter_hypothesis_stub".to_string()),
        handler: None,
    });

    // Sequential trace checkpoint is a core strict-reasoning primitive.
    specs.push(CommandSpec {
        cmd: "think.trace.sequential.step".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.trace.sequential.step".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases: vec!["trace.sequential.step".to_string()],
        handler_name: Some("trace_sequential_step".to_string()),
        handler: None,
    });

    // Auto-map remaining non-task handlers into think op=call surface.
    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if handlers::should_skip_handler_name(name) {
            continue;
        }

        let cmd = handlers::handler_think_cmd(name);
        let op_aliases = Vec::<String>::new();

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::ThinkOps,
            tier: Tier::Advanced,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: false,
                confirm_level: ConfirmLevel::None,
                idempotent: matches!(
                    name,
                    "think_query" | "think_pack" | "think_next" | "think_frontier" | "think_lint"
                ),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases,
            handler_name: Some(name.to_string()),
            handler: None,
        });
    }
}
