#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName,
};

use serde_json::json;

use super::handlers;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // v1 curated commands (custom UX layer).
    specs.push(CommandSpec {
        cmd: "think.knowledge.upsert".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.upsert".to_string(),
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
                    "anchor": { "type": "string" },
                    "key": { "type": "string", "description": "Stable knowledge key slug (enables evolvable upsert)." },
                    "key_mode": { "type": "string", "enum": ["explicit", "auto"] },
                    "lint_mode": { "type": "string", "enum": ["manual", "auto"] },
                    "card": { "type": ["object", "string"] },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["card"]
            }),
            example_minimal_args: json!({
                "anchor": "core",
                "key": "determinism",
                "card": { "title": "Invariant", "text": "Claim: ... / Apply: ... / Proof: ... / Expiry: ..." }
            }),
        },
        op_aliases: vec!["knowledge.upsert".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_knowledge_upsert),
    });

    specs.push(CommandSpec {
        cmd: "think.knowledge.key.suggest".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Advanced,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.key.suggest".to_string(),
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
                    "anchor": { "type": "string" },
                    "title": { "type": "string" },
                    "text": { "type": "string" },
                    "card": { "type": ["object", "string"] },
                    "limit": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({
                "anchor": "core",
                "title": "Determinism invariants"
            }),
        },
        op_aliases: vec!["knowledge.key.suggest".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_knowledge_key_suggest),
    });

    super::note_promote::register(specs);

    specs.push(CommandSpec {
        cmd: "think.knowledge.query".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.query".to_string(),
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
                    "anchor": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "key": { "type": "string", "description": "Filter to a single knowledge key (k:<slug> or <slug>)." },
                    "limit": { "type": "integer" },
                    "include_drafts": { "type": "boolean", "description": "Include draft-lane knowledge (default true for query)." },
                    "include_history": { "type": "boolean", "description": "When true, return historical versions; when false (default), latest-only." }
                },
                "required": []
            }),
            example_minimal_args: json!({ "limit": 12 }),
        },
        op_aliases: vec!["knowledge.query".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_knowledge_query),
    });

    super::knowledge_search::register(specs);

    specs.push(CommandSpec {
        cmd: "think.knowledge.recall".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.recall".to_string(),
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
                    "anchor": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ],
                        "description": "Anchor slug(s) or a:<slug> (recall is anchor-first)."
                    },
                    "limit": { "type": "integer" },
                    "text": { "type": "string" },
                    "include_drafts": { "type": "boolean" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({
                "anchor": "core",
                "limit": 12
            }),
        },
        op_aliases: vec!["knowledge.recall".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_knowledge_recall),
    });

    specs.push(CommandSpec {
        cmd: "think.knowledge.lint".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.lint".to_string(),
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
                    "scope": { "type": "string", "description": "Legacy/compat knob. Prefer anchor filter + limit." },
                    "limit": { "type": "integer", "description": "Max knowledge key index rows to scan (budget-capped)." },
                    "anchor": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ],
                        "description": "Optional anchor slug(s) to restrict lint. Same format as think.knowledge.recall."
                    },
                    "include_drafts": { "type": "boolean", "description": "Include draft-lane knowledge (default true)." },
                    "max_chars": { "type": "integer", "description": "Output budget (clamped by budget profile)." }
                },
                "required": []
            }),
            example_minimal_args: json!({ "anchor": "core" }),
        },
        op_aliases: vec!["knowledge.lint".to_string()],
        handler_name: None,
        handler: Some(handlers::handle_knowledge_lint),
    });

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
                    "think_query"
                        | "think_pack"
                        | "think_next"
                        | "think_frontier"
                        | "think_lint"
                        | "knowledge_list"
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
