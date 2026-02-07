#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::json::job_row_to_json;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_POLL_MS: u64 = 200;
const MIN_POLL_MS: u64 = 10;
const MAX_POLL_MS: u64 = 5_000;

fn is_terminal(status: &str) -> bool {
    matches!(status, "DONE" | "FAILED" | "CANCELED")
}

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

fn optional_u64(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<u64>, OpError> {
    let Some(v) = args_obj.get(key) else {
        return Ok(None);
    };
    match v {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| OpError {
                code: "INVALID_INPUT".to_string(),
                message: format!("{key}: expected non-negative integer"),
                recovery: Some(format!("Provide args.{key} as an integer >= 0")),
            })
            .map(Some),
        _ => Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected integer"),
            recovery: Some(format!("Provide args.{key} as an integer >= 0")),
        }),
    }
}

pub(super) fn handle_jobs_wait(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
    let timeout_ms = match optional_u64(args_obj, "timeout_ms") {
        Ok(v) => v.unwrap_or(DEFAULT_TIMEOUT_MS),
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    if timeout_ms > MAX_TIMEOUT_MS {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "timeout_ms exceeds max".to_string(),
                recovery: Some(format!("Use timeout_ms <= {MAX_TIMEOUT_MS}")),
            },
        );
    }
    let poll_ms = match optional_u64(args_obj, "poll_ms") {
        Ok(v) => v.unwrap_or(DEFAULT_POLL_MS),
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    if poll_ms > MAX_POLL_MS {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "poll_ms exceeds max".to_string(),
                recovery: Some(format!("Use poll_ms <= {MAX_POLL_MS}")),
            },
        );
    }
    if timeout_ms > 0 && poll_ms < MIN_POLL_MS {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "poll_ms is too small".to_string(),
                recovery: Some(format!("Use poll_ms >= {MIN_POLL_MS}")),
            },
        );
    }

    let started = Instant::now();
    let mut job = match server
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
    let mut done = is_terminal(job.status.as_str());

    while !done && timeout_ms > 0 {
        let elapsed_ms = started.elapsed().as_millis() as u64;
        if elapsed_ms >= timeout_ms {
            break;
        }
        let remaining_ms = timeout_ms.saturating_sub(elapsed_ms);
        let sleep_ms = poll_ms.min(remaining_ms);
        if sleep_ms == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(sleep_ms));

        job = match server
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
        done = is_terminal(job.status.as_str());
    }

    let waited_ms = started.elapsed().as_millis() as u64;

    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "done": done,
            "waited_ms": waited_ms,
            "job": job_row_to_json(job)
        }),
    );

    if !done {
        resp.actions.push(Action {
            action_id: "next::jobs.open".to_string(),
            priority: ActionPriority::Medium,
            tool: ToolName::JobsOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "open",
                "args": { "job": job_id },
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: "Истёк timeout: открыть job и проверить прогресс/состояние.".to_string(),
            risk: "Низкий".to_string(),
        });
    }

    resp
}
