#![forbid(unsafe_code)]

use crate::ops::{OpError, OpResponse};
use serde_json::Value;

pub(crate) fn handle(server: &mut crate::McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
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

    let workspace = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Delegate to the proven legacy open implementation (it is already budget-aware).
    let legacy = crate::tools::dispatch_tool(server, "open", args)
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "legacy open dispatch failed"));
    crate::ops::legacy_to_op_response("open", workspace.as_deref(), legacy).into_value()
}
