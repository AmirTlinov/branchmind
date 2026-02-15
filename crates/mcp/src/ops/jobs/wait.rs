#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::json::{job_event_to_json, job_row_to_json};

const DEFAULT_TIMEOUT_MS: u64 = 20_000;
// Keep jobs.wait below common MCP transport/tool-call deadlines (~30s), otherwise a single wait call
// can block the server thread and make subsequent calls look "hung".
const MAX_TIMEOUT_MS: u64 = 25_000;
const DEFAULT_POLL_MS: u64 = 200;
const MIN_POLL_MS: u64 = 20;
const MAX_POLL_MS: u64 = 5_000;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

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

fn optional_i64(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<i64>, OpError> {
    let Some(v) = args_obj.get(key) else {
        return Ok(None);
    };
    match v {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| OpError {
                code: "INVALID_INPUT".to_string(),
                message: format!("{key}: expected integer"),
                recovery: Some(format!("Provide args.{key} as an integer")),
            })
            .map(Some),
        _ => Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected integer"),
            recovery: Some(format!("Provide args.{key} as an integer")),
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

    if server.jobs_unknown_args_fail_closed_enabled {
        // NOTE: parse_envelope injects `workspace` into env.args for storage-layer convenience.
        const IMPLICIT_ENVELOPE_KEYS: &[&str] = &["workspace", "context_budget", "max_chars"];
        let allowed = [
            "job",
            "timeout_ms",
            "poll_ms",
            "mode",
            "after_seq",
            "limit",
            "max_events",
        ];
        let mut unknown = args_obj
            .keys()
            .filter(|k| {
                !allowed.iter().any(|a| a == &k.as_str())
                    && !IMPLICIT_ENVELOPE_KEYS.iter().any(|ik| ik == &k.as_str())
            })
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
                        "Remove unknown args or inspect schema via system op=schema.get (cmd=jobs.wait)."
                            .to_string(),
                    ),
                },
            );
        }
    }

    let job_id = match require_string(args_obj, "job") {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };

    let mode_raw = args_obj.get("mode").and_then(|v| v.as_str());
    let mode = mode_raw.unwrap_or(if server.jobs_wait_stream_v2_enabled {
        "stream"
    } else {
        "poll"
    });
    let mode = mode.trim();

    if mode.eq_ignore_ascii_case("stream") {
        if !server.jobs_wait_stream_v2_enabled {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "mode=stream is disabled".to_string(),
                    recovery: Some(
                        "Enable BRANCHMIND_JOBS_WAIT_STREAM_V2=1 or use mode=poll.".to_string(),
                    ),
                },
            );
        }

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
                    recovery: Some(format!(
                        "Use timeout_ms <= {MAX_TIMEOUT_MS}. For longer waits, loop jobs.wait (or use jobs.radar)."
                    )),
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
        let after_seq = match optional_i64(args_obj, "after_seq") {
            Ok(v) => v.unwrap_or(0),
            Err(err) => return OpResponse::error(env.cmd.clone(), err),
        };
        if after_seq < 0 {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "after_seq must be >= 0".to_string(),
                    recovery: Some("Provide args.after_seq as an integer >= 0".to_string()),
                },
            );
        }
        let max_events = match optional_u64(args_obj, "max_events") {
            Ok(v) => v,
            Err(err) => return OpResponse::error(env.cmd.clone(), err),
        };
        let limit = match optional_u64(args_obj, "limit") {
            Ok(v) => v,
            Err(err) => return OpResponse::error(env.cmd.clone(), err),
        };
        let max_events = match (max_events, limit) {
            (Some(a), Some(b)) if a != b => {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: "provide max_events or limit, not both".to_string(),
                        recovery: Some("Prefer max_events (stream v2) and omit limit.".to_string()),
                    },
                );
            }
            (Some(v), Some(_)) => v,
            (Some(v), None) => v,
            (None, Some(v)) => v,
            (None, None) => DEFAULT_LIMIT as u64,
        };
        let max_events = (max_events as usize).clamp(1, MAX_LIMIT);

        let started = Instant::now();

        let (done, job, tail, timed_out) = loop {
            // 1) Read job status.
            let j = match server
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
            let done = is_terminal(j.status.as_str());

            // 2) Tail events since after_seq.
            let t = match server.store.job_events_tail(
                &workspace,
                bm_storage::JobEventsTailRequest {
                    id: job_id.clone(),
                    after_seq,
                    limit: max_events,
                },
            ) {
                Ok(v) => v,
                Err(crate::StoreError::UnknownId) => {
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
            let has_new = !t.events.is_empty() || t.has_more || t.next_after_seq != after_seq;

            // Return immediately on any progress (new event(s) or terminal status).
            if done || has_new || timeout_ms == 0 {
                break (done, j, t, false);
            }

            let elapsed_ms = started.elapsed().as_millis() as u64;
            if elapsed_ms >= timeout_ms {
                break (done, j, t, true);
            }
            let remaining_ms = timeout_ms.saturating_sub(elapsed_ms);
            let sleep_ms = poll_ms.min(remaining_ms);
            if sleep_ms == 0 {
                break (done, j, t, true);
            }
            std::thread::sleep(Duration::from_millis(sleep_ms));
        };

        let waited_ms = started.elapsed().as_millis() as u64;

        let events_json = tail
            .events
            .into_iter()
            .map(job_event_to_json)
            .collect::<Vec<_>>();

        let mut resp = OpResponse::success(
            env.cmd.clone(),
            json!({
                "workspace": workspace.as_str(),
                "done": done,
                "timed_out": timed_out,
                "waited_ms": waited_ms,
                "job": job_row_to_json(job),
                "after_seq": tail.after_seq,
                "next_after_seq": tail.next_after_seq,
                "has_more": tail.has_more,
                "events": events_json
            }),
        );

        if timed_out {
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
                why: "Истёк timeout или нет новых событий: открыть job и проверить прогресс/состояние.".to_string(),
                risk: "Низкий".to_string(),
            });
        }

        return resp;
    }

    if !mode.eq_ignore_ascii_case("poll") {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "mode must be one of: stream|poll".to_string(),
                recovery: Some(
                    "Use mode=stream (default) or mode=poll (legacy status-only).".to_string(),
                ),
            },
        );
    }

    // v1 fallback: bounded polling (status-only).
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
                recovery: Some(format!(
                    "Use timeout_ms <= {MAX_TIMEOUT_MS}. For longer waits, loop jobs.wait (or use jobs.radar)."
                )),
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
    let timed_out = !done && timeout_ms > 0 && waited_ms >= timeout_ms;

    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "done": done,
            "timed_out": timed_out,
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
