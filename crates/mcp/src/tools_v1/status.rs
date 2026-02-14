#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, OpError, OpResponse};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

pub(crate) fn handle(server: &mut crate::McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return OpResponse::error(
            "status".to_string(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "arguments must be an object".to_string(),
                recovery: Some("Provide a JSON object".to_string()),
            },
        )
        .into_value();
    };

    let workspace = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| server.workspace_override.clone())
        .or_else(|| server.default_workspace.clone());

    let Some(workspace_str) = workspace.as_deref() else {
        let mut resp = OpResponse::error(
            "status".to_string(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "workspace is required".to_string(),
                recovery: Some("Provide workspace or call workspace op=use.".to_string()),
            },
        );
        resp.actions.push(Action {
            action_id: "recover::workspace.use".to_string(),
            priority: ActionPriority::High,
            tool: "workspace".to_string(),
            args: json!({ "op": "use", "args": { "workspace": "<workspace>" } }),
            why: "Установить workspace для сессии.".to_string(),
            risk: "Низкий".to_string(),
        });
        return resp.into_value();
    };

    let workspace_id = match crate::WorkspaceId::try_new(workspace_str.to_string()) {
        Ok(v) => v,
        Err(_) if looks_like_workspace_path(workspace_str) => {
            let resolved = match server.workspace_id_from_path_store(workspace_str) {
                Ok(v) => v,
                Err(crate::StoreError::InvalidInput(msg)) => {
                    return OpResponse::error(
                        "status".to_string(),
                        OpError {
                            code: "INVALID_INPUT".to_string(),
                            message: msg.to_string(),
                            recovery: None,
                        },
                    )
                    .into_value();
                }
                Err(err) => {
                    return OpResponse::error(
                        "status".to_string(),
                        OpError {
                            code: "STORE_ERROR".to_string(),
                            message: crate::format_store_error(err),
                            recovery: None,
                        },
                    )
                    .into_value();
                }
            };
            crate::WorkspaceId::try_new(resolved).unwrap_or_else(|_| {
                crate::WorkspaceId::try_new("my-workspace".to_string()).expect("fallback")
            })
        }
        Err(_) => {
            let suggested = suggest_workspace_id(workspace_str);
            let mut resp = OpResponse::error(
                "status".to_string(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some(format!(
                        "workspace must be a WorkspaceId (e.g. \"money1\"). You may also pass an absolute path (e.g. \"/home/me/repo\") and it will be mapped to an id. Fix: workspace=\"{suggested}\" (or call workspace op=use)."
                    )),
                },
            );
            resp.actions.push(Action {
                action_id: "recover::workspace.use.suggested".to_string(),
                priority: ActionPriority::High,
                tool: "workspace".to_string(),
                args: json!({ "op": "use", "args": { "workspace": suggested } }),
                why: "Установить workspace для сессии.".to_string(),
                risk: "Низкий".to_string(),
            });
            return resp.into_value();
        }
    };

    // Ensure workspace exists (v1 portal convenience).
    match server.store.workspace_exists(&workspace_id) {
        Ok(true) => {}
        Ok(false) => {
            if let Err(err) = server.store.workspace_init(&workspace_id) {
                return OpResponse::error(
                    "status".to_string(),
                    OpError {
                        code: "STORE_ERROR".to_string(),
                        message: crate::format_store_error(err),
                        recovery: None,
                    },
                )
                .into_value();
            }
        }
        Err(err) => {
            return OpResponse::error(
                "status".to_string(),
                OpError {
                    code: "STORE_ERROR".to_string(),
                    message: crate::format_store_error(err),
                    recovery: None,
                },
            )
            .into_value();
        }
    }
    if let Some(resp) = server.enforce_project_guard(&workspace_id) {
        return crate::ops::handler_to_op_response("status", Some(workspace_id.as_str()), resp)
            .into_value();
    }

    let report = crate::ops::derive_next(server, &workspace_id);
    let now_ms = crate::support::now_ms_i64();
    let inbox = server.store.jobs_status_counts(&workspace_id).ok();
    let runner = server
        .store
        .runner_status_snapshot(&workspace_id, now_ms)
        .ok();
    let runner_autostart_enabled = server.runner_autostart_enabled.load(Ordering::Relaxed);

    let mut out = OpResponse::success(
        "status".to_string(),
        json!({
            "server": {
                "version": crate::SERVER_VERSION,
                "build_fingerprint": crate::build_fingerprint(),
            },
            "workspace": workspace_id.as_str(),
            "headline": report.headline,
            "focus": report.focus_id,
            "checkout": report.checkout,
            "state_fingerprint": report.state_fingerprint,
            "jobs": {
                "queued": inbox.as_ref().map(|v| v.queued).unwrap_or(0),
                "running": inbox.as_ref().map(|v| v.running).unwrap_or(0)
            },
            "runner": runner.as_ref().map(|s| json!({
                "status": s.status,
                "live_count": s.live_count,
                "idle_count": s.idle_count,
                "offline_count": s.offline_count,
                "runner_id": s.runner_id,
                "active_job_id": s.active_job_id,
                "lease_expires_at_ms": s.lease_expires_at_ms
            })).unwrap_or(Value::Null),
            "runner_autostart": {
                "enabled": runner_autostart_enabled
            }
        }),
    );
    out.refs = report.refs;
    out.actions = report.actions;
    out.into_value()
}

fn suggest_workspace_id(raw: &str) -> String {
    let raw = raw.trim();
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in base.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
            continue;
        }
        if matches!(lc, '-' | '_' | '.' | ' ') {
            if !out.is_empty() && !prev_dash {
                out.push('-');
                prev_dash = true;
            }
            continue;
        }
        // Drop other chars, but keep a single '-' separator when meaningful.
        if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let mut suggested = if trimmed.is_empty() {
        "my-workspace".to_string()
    } else {
        trimmed.to_string()
    };
    if suggested.len() > 64 {
        suggested.truncate(64);
        suggested = suggested.trim_matches('-').to_string();
        if suggested.is_empty() {
            return "my-workspace".to_string();
        }
    }
    suggested
}

fn looks_like_workspace_path(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.is_empty() {
        return false;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return true;
    }
    if raw == "." || raw == ".." || raw.starts_with("./") || raw.starts_with("../") {
        return true;
    }
    if raw == "~" || raw.starts_with("~/") {
        return true;
    }
    if raw.contains('\\') {
        return true;
    }
    // Windows drive path: "C:\..." / "C:/..."
    if raw.len() >= 2 && raw.as_bytes().get(1) == Some(&b':') {
        return true;
    }
    false
}
