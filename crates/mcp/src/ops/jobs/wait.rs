#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::json::job_row_to_json;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
// Flagship UX / transport safety:
// MCP clients and shared proxy layers often enforce per-call deadlines (~20–60s).
// `jobs.wait` must remain safe to call with large timeout_ms, so we clamp the blocking portion
// per call to a small deterministic bound.
const MAX_BLOCKING_PER_CALL_MS: u64 = 2_000;
const DEFAULT_POLL_MS: u64 = 200;
const MIN_POLL_MS: u64 = 10;
const MAX_POLL_MS: u64 = 5_000;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WaitOutputMode {
    Default,
    Watch,
}

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
            recovery: Some(format!("Provide args.{key} as a string")),
        }),
    }
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

    let mode = match optional_string(args_obj, "mode") {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };
    let mode = match mode.as_deref() {
        None | Some("default") => WaitOutputMode::Default,
        Some("watch") => WaitOutputMode::Watch,
        Some(other) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: format!("mode: unknown value {other:?}"),
                    recovery: Some(
                        "Use mode=\"default\" (structured) or mode=\"watch\" (1-2 lines)"
                            .to_string(),
                    ),
                },
            );
        }
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
    let requested_timeout_ms = timeout_ms;
    let call_timeout_ms = requested_timeout_ms.min(MAX_BLOCKING_PER_CALL_MS);
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
    if call_timeout_ms > 0 && poll_ms < MIN_POLL_MS {
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

    while !done && call_timeout_ms > 0 {
        let elapsed_ms = started.elapsed().as_millis() as u64;
        if elapsed_ms >= call_timeout_ms {
            break;
        }
        let remaining_ms = call_timeout_ms.saturating_sub(elapsed_ms);
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

    if mode == WaitOutputMode::Watch {
        let remaining_ms = requested_timeout_ms.saturating_sub(waited_ms);
        let stop_hint = "stop: done=true (DONE|FAILED|CANCELED)";
        let hint = match job.status.as_str() {
            "QUEUED" => "hint: jobs.runner.start / jobs.open / system.daemon.restart",
            "RUNNING" => "hint: jobs.open / jobs.tail / system.daemon.restart",
            _ => "hint: jobs.open / system.daemon.restart",
        };

        let mut lines = Vec::<String>::new();
        lines.push(format!(
            "job={job_id} status={} done={} waited={}ms eff={}ms remaining~{}ms | {stop_hint} | {hint}",
            job.status,
            done,
            waited_ms,
            call_timeout_ms,
            remaining_ms
        ));

        if !done {
            // Copy/paste‑valid continuation: repeats the same bounded wait with mode=watch so an
            // agent can loop without thinking.
            let args_json = json!({
                "job": job_id,
                "timeout_ms": requested_timeout_ms,
                "poll_ms": poll_ms,
                "mode": "watch"
            });
            let args_str = serde_json::to_string(&args_json).unwrap_or_else(|_| "{}".to_string());
            lines.push(format!(
                "jobs workspace={} op=wait args={} budget_profile=portal view=compact",
                workspace.as_str(),
                args_str
            ));
        }

        let mut resp = OpResponse::success(env.cmd.clone(), Value::String(lines.join("\n")));
        resp.line_protocol = true;
        return resp;
    }

    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "done": done,
            "waited_ms": waited_ms,
            "requested_timeout_ms": requested_timeout_ms,
            "effective_timeout_ms": call_timeout_ms,
            "remaining_ms": requested_timeout_ms.saturating_sub(waited_ms),
            "job": job_row_to_json(job)
        }),
    );

    if !done {
        resp.actions.push(Action {
            action_id: "next::jobs.wait".to_string(),
            priority: ActionPriority::High,
            tool: ToolName::JobsOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "call",
                "cmd": "jobs.wait",
                "args": {
                    "job": job_id.clone(),
                    "timeout_ms": requested_timeout_ms,
                    "poll_ms": poll_ms
                },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Job не завершён: подождать ещё (bounded wait) и повторить проверку.".to_string(),
            risk: "Низкий".to_string(),
        });
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
            why: "Job не завершён: открыть job и проверить прогресс/состояние.".to_string(),
            risk: "Низкий".to_string(),
        });
    }

    resp
}
