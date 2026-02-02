#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::{Value, json};

use super::json::{job_event_to_json, job_row_to_json};

fn require_string(args_obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, OpError> {
    let Some(v) = args_obj.get(key).and_then(|v| v.as_str()) else {
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key} is required"),
            recovery: Some(format!("Provide args.{key} as a string")),
        });
    };
    Ok(v.to_string())
}

fn optional_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, OpError> {
    let Some(v) = args_obj.get(key) else {
        return Ok(None);
    };
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.to_string())),
        _ => Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected string"),
            recovery: Some(format!("Provide args.{key} as a string (or omit it)")),
        }),
    }
}

fn optional_string_array(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, OpError> {
    let Some(v) = args_obj.get(key) else {
        return Ok(Vec::new());
    };
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let Some(s) = item.as_str() else {
                    return Err(OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: format!("{key}: expected string array"),
                        recovery: Some(format!("Provide args.{key} as an array of strings")),
                    });
                };
                out.push(s.to_string());
            }
            Ok(out)
        }
        _ => Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected string array"),
            recovery: Some(format!("Provide args.{key} as an array of strings")),
        }),
    }
}

pub(super) fn handle_jobs_cancel(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={job:\"JOB-...\"}".to_string()),
            },
        );
    };

    let job_id = match require_string(args_obj, "job") {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    let reason = match optional_string(args_obj, "reason") {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    let mut refs = match optional_string_array(args_obj, "refs") {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    let meta_value = args_obj.get("meta").cloned().filter(|v| !v.is_null());
    let meta_json = meta_value
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());

    // Keep job thread navigable even when callers omit refs (bounded, deterministic).
    if refs.len() < 32 && !refs.iter().any(|r| r == &job_id) {
        refs.push(job_id.clone());
    }

    let job = match server
        .store
        .job_get(&workspace, bm_storage::JobGetRequest { id: job_id.clone() })
    {
        Ok(Some(v)) => v,
        Ok(None) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "UNKNOWN_ID".to_string(),
                    message: "Unknown job id".to_string(),
                    recovery: Some(
                        "Call jobs op=radar to list known jobs, then retry.".to_string(),
                    ),
                },
            );
        }
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

    match job.status.as_str() {
        "QUEUED" => {}
        "RUNNING" => {
            let mut resp = OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "CONFLICT".to_string(),
                    message: format!("job is RUNNING and cannot be canceled directly (job_id={job_id})"),
                    recovery: Some(
                        "Use jobs.complete status=CANCELED (requires runner_id + claim_revision), or wait for completion."
                            .to_string(),
                    ),
                },
            );

            // Always include a cheap open action for inspection.
            resp.actions.push(Action {
                action_id: "next::jobs.open".to_string(),
                priority: ActionPriority::Medium,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "open",
                    "args": { "job": job_id },
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Открыть job и проверить текущий статус/runner.".to_string(),
                risk: "Низкий".to_string(),
            });

            if let Some(runner_id) = job.runner.as_deref() {
                let mut args = json!({
                    "job": job_id,
                    "runner_id": runner_id,
                    "claim_revision": job.revision,
                    "status": "CANCELED",
                });
                if let Some(reason) = reason.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    && let Some(obj) = args.as_object_mut()
                {
                    obj.insert("summary".to_string(), Value::String(reason.to_string()));
                }
                if !refs.is_empty()
                    && let Some(obj) = args.as_object_mut()
                {
                    obj.insert("refs".to_string(), json!(refs));
                }
                if let Some(meta) = meta_value
                    && let Some(obj) = args.as_object_mut()
                {
                    obj.insert("meta".to_string(), meta);
                }

                resp.actions.push(Action {
                    action_id: "recover::jobs.complete.canceled".to_string(),
                    priority: ActionPriority::High,
                    tool: ToolName::JobsOps.as_str().to_string(),
                    args: json!({
                        "workspace": workspace.as_str(),
                        "op": "call",
                        "cmd": "jobs.complete",
                        "args": args,
                        "budget_profile": "portal",
                        "view": "compact"
                    }),
                    why: "Принудительно завершить RUNNING job как CANCELED (через complete)."
                        .to_string(),
                    risk: "Средний: runner мог ещё выполнять задачу; завершение отменой обрывает контракт."
                        .to_string(),
                });
            } else {
                resp.warnings.push(crate::warning(
                    "RUNNER_ID_MISSING",
                    "job is RUNNING but runner_id is missing; cannot prefill jobs.complete",
                    "Open the job to inspect runner info and then complete it manually.",
                ));
            }

            return resp;
        }
        "DONE" | "FAILED" | "CANCELED" => {
            let mut resp = OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "CONFLICT".to_string(),
                    message: format!(
                        "job is already terminal and cannot be canceled (job_id={job_id}, status={})",
                        job.status
                    ),
                    recovery: Some("Open the job to inspect prior completion.".to_string()),
                },
            );
            resp.actions.push(Action {
                action_id: "next::jobs.open".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "open",
                    "args": { "job": job_id },
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Открыть job для просмотра результата/refs.".to_string(),
                risk: "Низкий".to_string(),
            });
            return resp;
        }
        other => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "CONFLICT".to_string(),
                    message: format!("unexpected job status: {other}"),
                    recovery: Some("Open the job to inspect its state.".to_string()),
                },
            );
        }
    }

    let canceled = match server.store.job_cancel(
        &workspace,
        bm_storage::JobCancelRequest {
            id: job_id.clone(),
            reason,
            refs,
            meta_json,
        },
    ) {
        Ok(v) => v,
        Err(crate::StoreError::UnknownId) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "UNKNOWN_ID".to_string(),
                    message: "Unknown job id".to_string(),
                    recovery: Some("Call jobs op=radar to list jobs, then retry.".to_string()),
                },
            );
        }
        Err(crate::StoreError::JobAlreadyTerminal { job_id, status }) => {
            let mut resp = OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "CONFLICT".to_string(),
                    message: format!("job already terminal (job_id={job_id}, status={status})"),
                    recovery: Some(
                        "Open the job to inspect completion and referenced artifacts.".to_string(),
                    ),
                },
            );
            resp.actions.push(Action {
                action_id: "next::jobs.open".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "open",
                    "args": { "job": job_id },
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Открыть job для просмотра результата/refs.".to_string(),
                risk: "Низкий".to_string(),
            });
            return resp;
        }
        Err(crate::StoreError::JobNotCancelable { job_id, status }) => {
            let mut resp = OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "CONFLICT".to_string(),
                    message: format!("job is not cancelable (job_id={job_id}, status={status})"),
                    recovery: Some(
                        "Cancel only works for QUEUED jobs. Open the job to inspect state."
                            .to_string(),
                    ),
                },
            );
            resp.actions.push(Action {
                action_id: "next::jobs.open".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "open",
                    "args": { "job": job_id },
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Открыть job и проверить текущий статус.".to_string(),
                risk: "Низкий".to_string(),
            });
            return resp;
        }
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

    OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(canceled.job),
            "event": job_event_to_json(canceled.event)
        }),
    )
}
