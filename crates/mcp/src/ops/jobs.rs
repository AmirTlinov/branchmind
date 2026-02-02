#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Envelope, OpError, OpResponse, Safety,
    SchemaSource, Stability, Tier, ToolName, legacy_to_cmd_segments,
};
use serde_json::json;

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
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
        legacy_tool: None,
        handler: Some(handle_runner_start),
    });

    for def in crate::tools::tool_definitions(crate::Toolset::Full) {
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
                schema: SchemaSource::Legacy,
                op_aliases: Vec::new(),
                legacy_tool: Some(name.to_string()),
                handler: None,
            });
            continue;
        }

        if !name.starts_with("tasks_jobs_") {
            continue;
        }
        let suffix = &name["tasks_jobs_".len()..];
        let cmd = format!("jobs.{}", legacy_to_cmd_segments(suffix));

        let mut op_aliases = Vec::<String>::new();
        if matches!(suffix, "create" | "list" | "radar" | "open") {
            op_aliases.push(suffix.to_string());
        }

        let doc_ref_anchor = if matches!(suffix, "create" | "list" | "radar" | "open") {
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
                destructive: matches!(suffix, "complete" | "requeue"),
                confirm_level: if matches!(suffix, "complete" | "requeue") {
                    ConfirmLevel::Soft
                } else {
                    ConfirmLevel::None
                },
                idempotent: matches!(suffix, "list" | "radar" | "open" | "tail"),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Legacy,
            op_aliases,
            legacy_tool: Some(name.to_string()),
            handler: None,
        });
    }
}

fn handle_runner_start(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    let now_ms = crate::support::now_ms_i64();
    let runner_status_before = match server.store.runner_status_snapshot(&workspace, now_ms) {
        Ok(v) => v,
        Err(crate::StoreError::InvalidInput(msg)) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: None,
                },
            );
        }
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "STORE_ERROR".to_string(),
                    message: crate::format_store_error(err),
                    recovery: None,
                },
            );
        }
    };
    let runner_is_offline = runner_status_before.status.as_str() == "offline";

    let counts = server.store.jobs_status_counts(&workspace).ok();
    let queued = counts.as_ref().map(|c| c.queued).unwrap_or(0);
    let running = counts.as_ref().map(|c| c.running).unwrap_or(0);

    if !runner_is_offline {
        return OpResponse::success(
            env.cmd.clone(),
            json!({
                "workspace": workspace.as_str(),
                "attempted": false,
                "active": true,
                "jobs": { "queued": queued, "running": running },
                "runner": {
                    "status": runner_status_before.status,
                    "live_count": runner_status_before.live_count,
                    "idle_count": runner_status_before.idle_count,
                    "offline_count": runner_status_before.offline_count,
                    "runner_id": runner_status_before.runner_id,
                    "active_job_id": runner_status_before.active_job_id,
                    "lease_expires_at_ms": runner_status_before.lease_expires_at_ms
                }
            }),
        );
    }

    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "attempted": true,
            "active": false,
            "jobs": { "queued": queued, "running": running },
            "runner_before": {
                "status": runner_status_before.status,
                "live_count": runner_status_before.live_count,
                "idle_count": runner_status_before.idle_count,
                "offline_count": runner_status_before.offline_count,
                "runner_id": runner_status_before.runner_id,
                "active_job_id": runner_status_before.active_job_id,
                "lease_expires_at_ms": runner_status_before.lease_expires_at_ms
            },
            "runner_bootstrap": server.runner_bootstrap_json(&workspace)
        }),
    );

    match server.start_runner_on_demand(&workspace, now_ms) {
        Ok(true) => {
            if let Some(obj) = resp.result.as_object_mut() {
                obj.insert("active".to_string(), serde_json::Value::Bool(true));
            }
        }
        Ok(false) => {
            // This currently never returns false, but keep the contract future-proof.
        }
        Err(err) => {
            resp.warnings.push(crate::warning(
                "RUNNER_START_FAILED",
                &format!("runner start failed: {err}"),
                "Run the runner_bootstrap cmd (copy/paste) or configure bm_runner in PATH.",
            ));
        }
    }

    // Provide a cheap follow-up action (watch the queue / lease) when jobs are present.
    if queued > 0 || running > 0 {
        resp.actions.push(crate::ops::Action {
            action_id: "next::jobs.radar".to_string(),
            priority: crate::ops::ActionPriority::Medium,
            tool: ToolName::JobsOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "radar",
                "args": {},
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Проверить статус очереди и runner leases (radar).".to_string(),
            risk: "Низкий".to_string(),
        });
    }

    resp
}
