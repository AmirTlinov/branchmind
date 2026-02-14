#![forbid(unsafe_code)]

use crate::ops::{OpError, OpResponse};
use serde_json::Value;

pub(crate) fn handle(server: &mut crate::McpServer, args: Value) -> Value {
    let mut args = args;
    let Some(args_obj) = args.as_object_mut() else {
        return OpResponse::error(
            "open".to_string(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "arguments must be an object".to_string(),
                recovery: Some("Provide {id:\"...\"} (+ optional workspace/limit).".to_string()),
            },
        )
        .into_value();
    };
    let id = args_obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if id.is_empty() {
        return OpResponse::error(
            "open".to_string(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "id is required".to_string(),
                recovery: Some("Provide id (TASK-*/PLAN-*/CARD-*/notes@seq/etc).".to_string()),
            },
        )
        .into_value();
    }

    // DX: accept workspace as a filesystem path and bind it to a stable workspace id.
    if let Some(ws_raw) = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        && crate::WorkspaceId::try_new(ws_raw.clone()).is_err()
        && looks_like_workspace_path(&ws_raw)
    {
        let resolved = match server.workspace_id_from_path_store(&ws_raw) {
            Ok(v) => v,
            Err(crate::StoreError::InvalidInput(msg)) => {
                return OpResponse::error(
                    "open".to_string(),
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
                    "open".to_string(),
                    OpError {
                        code: "STORE_ERROR".to_string(),
                        message: crate::format_store_error(err),
                        recovery: None,
                    },
                )
                .into_value();
            }
        };
        args_obj.insert("workspace".to_string(), Value::String(resolved));
    }

    let workspace = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let handler_resp = crate::handlers::dispatch_handler(server, "open", args)
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "open dispatch failed"));
    crate::ops::handler_to_op_response("open", workspace.as_deref(), handler_resp).into_value()
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
