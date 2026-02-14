#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::json;

pub(super) fn handle_runner_start(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    if server.jobs_unknown_args_fail_closed_enabled {
        let Some(args_obj) = env.args.as_object() else {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "args must be an object".to_string(),
                    recovery: Some(
                        "Call jobs.runner.start with args={} (or omit args).".to_string(),
                    ),
                },
            );
        };
        // NOTE: parse_envelope injects `workspace` into env.args for storage-layer convenience.
        // Treat it as an implicit envelope key for strict unknown-args guards.
        const IMPLICIT_ENVELOPE_KEYS: &[&str] =
            &["workspace", "context_budget", "limit", "max_chars"];
        let mut unknown = args_obj
            .keys()
            .filter(|k| !IMPLICIT_ENVELOPE_KEYS.iter().any(|ik| ik == &k.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        unknown.sort();
        unknown.dedup();
        if !unknown.is_empty() {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: format!("unknown args: {}", unknown.join(", ")),
                    recovery: Some(
                        "Remove unknown args or inspect schema via system op=schema.get (cmd=jobs.runner.start)."
                            .to_string(),
                    ),
                },
            );
        }
    }

    let now_ms = crate::support::now_ms_i64();
    // Self-heal runner leases before any UX decisions: clear dangling/terminal active_job_id links.
    // This is best-effort (fail-open) to avoid blocking runner bootstrap on diagnostics.
    let mut self_heal_warning: Option<serde_json::Value> = None;
    let self_heal = match server
        .store
        .runner_leases_self_heal_active_job_links(&workspace, now_ms)
    {
        Ok(v) => Some(v),
        Err(err) => {
            self_heal_warning = Some(crate::warning(
                "RUNNER_LEASE_SELF_HEAL_FAILED",
                &format!("runner lease self-heal failed: {err}"),
                "Proceeding without self-heal; run jobs.radar for diagnostics.",
            ));
            None
        }
    };
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
        let mut resp = OpResponse::success(
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
        if let Some(sh) = self_heal
            && let Some(obj) = resp.result.as_object_mut()
        {
            obj.insert(
                "self_heal".to_string(),
                json!({ "inspected": sh.inspected, "cleared": sh.cleared }),
            );
        }
        if let Some(w) = self_heal_warning {
            resp.warnings.push(w);
        }
        return resp;
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
    if let Some(sh) = self_heal
        && let Some(obj) = resp.result.as_object_mut()
    {
        obj.insert(
            "self_heal".to_string(),
            json!({ "inspected": sh.inspected, "cleared": sh.cleared }),
        );
    }
    if let Some(w) = self_heal_warning {
        resp.warnings.push(w);
    }

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
        resp.actions.push(Action {
            action_id: "next::jobs.radar".to_string(),
            priority: ActionPriority::Medium,
            tool: ToolName::JobsOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "radar",
                "args": {},
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: "Проверить статус очереди и runner leases (radar).".to_string(),
            risk: "Низкий".to_string(),
        });
    }

    resp
}
