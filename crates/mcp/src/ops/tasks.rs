#![forbid(unsafe_code)]

use crate::ops::{
    BudgetCaps, BudgetPolicy, BudgetProfile, CommandSpec, ConfirmLevel, DocRef, Envelope, OpError,
    OpResponse, Safety, SchemaSource, Stability, Tier, ToolName, name_to_cmd_segments,
};
use serde_json::json;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // Mirror internal task handlers as cmd=tasks.<suffix>.
    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !name.starts_with("tasks_") {
            continue;
        }

        let suffix = &name["tasks_".len()..];
        let cmd = match suffix {
            "create" => "tasks.plan.create".to_string(),
            "decompose" => "tasks.plan.decompose".to_string(),
            "evidence_capture" => "tasks.evidence.capture".to_string(),
            "close_step" => "tasks.step.close".to_string(),
            "slices_propose_next" => "tasks.slices.propose_next".to_string(),
            "slices_apply" => "tasks.slices.apply".to_string(),
            "slice_open" => "tasks.slice.open".to_string(),
            "slice_validate" => "tasks.slice.validate".to_string(),
            _ => format!("tasks.{}", name_to_cmd_segments(suffix)),
        };
        let mut op_aliases = Vec::<String>::new();
        let mut tier = Tier::Advanced;
        let mut budget = BudgetPolicy::standard();

        match suffix {
            "create" => op_aliases.push("plan.create".to_string()),
            "decompose" => op_aliases.push("plan.decompose".to_string()),
            "evidence_capture" => op_aliases.push("evidence.capture".to_string()),
            "close_step" => op_aliases.push("step.close".to_string()),
            "planfs_init" => op_aliases.push("planfs.init".to_string()),
            "planfs_export" => op_aliases.push("planfs.export".to_string()),
            "planfs_import" => op_aliases.push("planfs.import".to_string()),
            "search" => {
                op_aliases.push("search".to_string());
                tier = Tier::Gold;
                budget = BudgetPolicy {
                    default_profile: BudgetProfile::Portal,
                    portal_caps: BudgetCaps {
                        max_chars: Some(6_000),
                        context_budget: Some(6_000),
                        limit: Some(12),
                    },
                    default_caps: BudgetCaps {
                        max_chars: Some(20_000),
                        context_budget: Some(20_000),
                        limit: Some(60),
                    },
                    audit_caps: BudgetCaps {
                        max_chars: Some(80_000),
                        context_budget: Some(80_000),
                        limit: Some(120),
                    },
                };
            }
            _ => {}
        }

        let doc_ref_anchor = if matches!(
            suffix,
            "create"
                | "decompose"
                | "evidence_capture"
                | "close_step"
                | "search"
                | "planfs_init"
                | "planfs_export"
                | "planfs_import"
                | "slices_propose_next"
                | "slices_apply"
                | "slice_open"
                | "slice_validate"
        ) {
            format!("#{cmd}")
        } else {
            "#cmd-index".to_string()
        };

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::TasksOps,
            tier,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: doc_ref_anchor,
            },
            safety: Safety {
                destructive: suffix.contains("delete"),
                confirm_level: if suffix.contains("delete") {
                    ConfirmLevel::Hard
                } else {
                    ConfirmLevel::None
                },
                idempotent: !suffix.contains("create") && !suffix.contains("delete"),
            },
            budget,
            schema: SchemaSource::Handler,
            op_aliases,
            handler_name: Some(name.to_string()),
            handler: None,
        });
    }

    // v1: tasks.execute.next (custom, NextEngine lens)
    specs.push(CommandSpec {
        cmd: "tasks.execute.next".to_string(),
        domain_tool: ToolName::TasksOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#tasks.execute.next".to_string(),
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
        op_aliases: vec!["execute.next".to_string()],
        handler_name: None,
        handler: Some(handle_execute_next),
    });

    // v1: tasks.exec.summary (one-command preset: exec summary + critical regressions)
    specs.push(CommandSpec {
        cmd: "tasks.exec.summary".to_string(),
        domain_tool: ToolName::TasksOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#tasks.exec.summary".to_string(),
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
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "target": { "type": "string" }
                },
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["exec.summary".to_string()],
        handler_name: None,
        handler: Some(handle_exec_summary),
    });
}

fn handle_execute_next(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(ws) = env.workspace.as_deref() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "workspace is required".to_string(),
                recovery: Some(
                    "Call workspace op=use first (or configure default workspace).".to_string(),
                ),
            },
        );
    };
    let workspace = match crate::WorkspaceId::try_new(ws.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some("Use workspace like my-workspace".to_string()),
                },
            );
        }
    };

    let report = crate::ops::derive_next(server, &workspace);
    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "headline": report.headline,
            "focus": report.focus_id,
            "state_fingerprint": report.state_fingerprint,
        }),
    );
    resp.refs = report.refs;
    resp.actions = report.actions;
    resp
}

fn handle_exec_summary(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    crate::ops::build_tasks_exec_summary(
        server,
        env.cmd.clone(),
        env.workspace.as_deref(),
        env.args.clone(),
    )
}
