#![forbid(unsafe_code)]

mod cancel;
mod exec_summary;
mod json;
mod runner_start;
mod wait;

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName, name_to_cmd_segments,
};
use serde_json::json;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // v1: jobs.exec.summary (one-command teamlead pulse, minimal/noise-first)
    specs.push(CommandSpec {
        cmd: "jobs.exec.summary".to_string(),
        domain_tool: ToolName::JobsOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#jobs.exec.summary".to_string(),
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
                    "view": { "type": "string", "enum": ["smart", "audit"] },
                    "limit": { "type": "integer" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "stall_after_s": { "type": "integer" },
                    "max_regressions": { "type": "integer" },
                    "include_details": { "type": "boolean" }
                },
                "required": [],
                "additionalProperties": false
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["exec.summary".to_string()],
        handler_name: None,
        handler: Some(exec_summary::handle_jobs_exec_summary),
    });

    // v1: jobs.runner.start (custom, explicit runner bootstrap)
    specs.push(CommandSpec {
        cmd: "jobs.runner.start".to_string(),
        domain_tool: ToolName::JobsOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#jobs.runner.start".to_string(),
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
        op_aliases: vec!["runner.start".to_string()],
        handler_name: None,
        handler: Some(runner_start::handle_runner_start),
    });

    // v1: jobs.cancel (custom, queued-only cancellation)
    specs.push(CommandSpec {
        cmd: "jobs.cancel".to_string(),
        domain_tool: ToolName::JobsOps,
        tier: Tier::Advanced,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#jobs.cancel".to_string(),
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
                    "job": { "type": "string" },
                    "reason": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" },
                    "force_running": { "type": "boolean" },
                    "expected_revision": { "type": "integer" }
                },
                "required": ["job"]
            }),
            example_minimal_args: json!({ "job": "<job>" }),
        },
        op_aliases: vec!["cancel".to_string()],
        handler_name: None,
        handler: Some(cancel::handle_jobs_cancel),
    });

    // v1: jobs.wait (custom, bounded polling)
    specs.push(CommandSpec {
        cmd: "jobs.wait".to_string(),
        domain_tool: ToolName::JobsOps,
        tier: Tier::Advanced,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#jobs.wait".to_string(),
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
                    "job": { "type": "string" },
                    "timeout_ms": { "type": "integer" },
                    "poll_ms": { "type": "integer" },
                    "mode": { "type": "string", "enum": ["stream", "poll"] },
                    "after_seq": { "type": "integer" },
                    "max_events": { "type": "integer" },
                    "limit": { "type": "integer", "description": "deprecated alias for max_events" }
                },
                "required": ["job"]
            }),
            example_minimal_args: json!({ "job": "<job>" }),
        },
        op_aliases: vec!["wait".to_string()],
        handler_name: None,
        handler: Some(wait::handle_jobs_wait),
    });

    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };

        if name == "tasks_runner_heartbeat" {
            specs.push(CommandSpec {
                cmd: "jobs.runner.heartbeat".to_string(),
                domain_tool: ToolName::JobsOps,
                tier: Tier::Advanced,
                stability: Stability::Stable,
                doc_ref: DocRef {
                    path: "docs/contracts/V1_COMMANDS.md".to_string(),
                    anchor: "#jobs.runner.heartbeat".to_string(),
                },
                safety: Safety {
                    destructive: false,
                    confirm_level: ConfirmLevel::None,
                    idempotent: true,
                },
                budget: BudgetPolicy::standard(),
                schema: SchemaSource::Handler,
                op_aliases: vec!["runner.heartbeat".to_string()],
                handler_name: Some(name.to_string()),
                handler: None,
            });
            continue;
        }

        if !name.starts_with("tasks_jobs_") {
            continue;
        }
        let suffix = &name["tasks_jobs_".len()..];
        let cmd = format!("jobs.{}", name_to_cmd_segments(suffix));

        // Jobs UX: expose every first-class jobs.* command as a direct op alias so
        // copy/paste never requires switching to op=call + cmd for common flows.
        let op_aliases = vec![name_to_cmd_segments(suffix)];

        let doc_ref_anchor = if matches!(
            suffix,
            "create"
                | "list"
                | "radar"
                | "open"
                | "artifact_put"
                | "artifact_get"
                | "complete"
                | "control_center"
                | "macro_dispatch_scout"
                | "macro_dispatch_builder"
                | "macro_dispatch_validator"
                | "pipeline_gate"
                | "pipeline_apply"
                | "pipeline_context_review"
                | "mesh_publish"
                | "mesh_pull"
                | "mesh_ack"
                | "mesh_link"
        ) {
            format!("#{cmd}")
        } else {
            "#cmd-index".to_string()
        };
        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::JobsOps,
            tier: Tier::Advanced,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: doc_ref_anchor,
            },
            safety: Safety {
                destructive: matches!(suffix, "complete" | "requeue" | "macro_rotate_stalled"),
                confirm_level: if matches!(suffix, "complete" | "requeue" | "macro_rotate_stalled")
                {
                    ConfirmLevel::Soft
                } else {
                    ConfirmLevel::None
                },
                idempotent: matches!(suffix, "list" | "radar" | "open" | "tail"),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases,
            handler_name: Some(name.to_string()),
            handler: None,
        });
    }
}
