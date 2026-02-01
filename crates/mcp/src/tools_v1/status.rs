#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, OpError, OpResponse};
use serde_json::{Value, json};

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
        Err(_) => {
            return OpResponse::error(
                "status".to_string(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some("Use workspace like my-workspace".to_string()),
                },
            )
            .into_value();
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
        return crate::ops::legacy_to_op_response("status", Some(workspace_id.as_str()), resp)
            .into_value();
    }

    let report = crate::ops::derive_next(server, &workspace_id);
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
        }),
    );
    out.refs = report.refs;
    out.actions = report.actions;
    out.into_value()
}
