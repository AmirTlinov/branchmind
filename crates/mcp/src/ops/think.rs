#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName, name_to_cmd_segments,
};
use serde_json::json;

const KB_BRANCH: &str = "kb/main";
const KB_GRAPH_DOC: &str = "kb-graph";
const KB_TRACE_DOC: &str = "kb-trace";

mod knowledge;
mod knowledge_lint;
mod note;
mod reasoning;

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
        handler: Some(knowledge::handle_knowledge_upsert),
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
        handler: Some(knowledge::handle_knowledge_key_suggest),
    });

    specs.push(CommandSpec {
        cmd: "think.note.promote".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Advanced,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.note.promote".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "note_ref": { "type": "string" },
                    "anchor": { "type": "string" },
                    "key": { "type": "string" },
                    "title": { "type": "string" },
                    "key_mode": { "type": "string", "enum": ["explicit", "auto"] },
                    "lint_mode": { "type": "string", "enum": ["manual", "auto"] }
                },
                "required": ["note_ref"]
            }),
            example_minimal_args: json!({
                "note_ref": "notes@123",
                "anchor": "core"
            }),
        },
        op_aliases: vec!["note.promote".to_string()],
        handler_name: None,
        handler: Some(note::handle_note_promote),
    });

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
        handler: Some(knowledge::handle_knowledge_query),
    });

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
        handler: Some(knowledge::handle_knowledge_recall),
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
        handler: Some(knowledge::handle_knowledge_lint),
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
        handler: Some(reasoning::handle_reasoning_seed),
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
        handler: Some(reasoning::handle_reasoning_pipeline),
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
        handler: Some(reasoning::handle_idea_branch_merge),
    });

    // Auto-map remaining non-task handlers into think op=call surface.
    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if should_skip_handler_name(name) {
            continue;
        }

        let cmd = handler_think_cmd(name);
        let op_aliases = Vec::<String>::new();
        let tier = if matches!(name, "think_card" | "think_playbook" | "macro_anchor_note")
            || name.starts_with("anchors_")
            || name.starts_with("anchor_")
        {
            Tier::Gold
        } else {
            Tier::Advanced
        };

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::ThinkOps,
            tier,
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

fn should_skip_handler_name(name: &str) -> bool {
    if name.starts_with("tasks_") {
        return true;
    }
    if name.starts_with("graph_") {
        return true;
    }
    if matches!(
        name,
        // Dedicated v1 portals:
        "status" | "open" | "workspace_use" | "workspace_reset"
            // System:
            | "storage" | "init" | "help" | "skill" | "diagnostics"
            // VCS / docs:
            | "branch_create" | "branch_list" | "checkout" | "branch_rename" | "branch_delete"
            | "notes_commit" | "commit" | "log" | "reflog" | "reset" | "show" | "diff" | "merge"
            | "tag_create" | "tag_list" | "tag_delete"
            | "docs_list" | "transcripts_search" | "transcripts_open" | "transcripts_digest"
            | "export"
            // Curated cmds (registered explicitly):
            | "macro_branch_note"
            | "knowledge_list"
            | "think_lint"
            | "think_template"
            | "think_pipeline"
    ) {
        return true;
    }
    false
}

fn handler_think_cmd(name: &str) -> String {
    if let Some(suffix) = name.strip_prefix("think_") {
        return format!("think.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchors_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchor_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    format!("think.{}", name_to_cmd_segments(name))
}
