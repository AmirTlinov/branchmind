#![forbid(unsafe_code)]

use crate::ops::{Envelope, OpResponse};

pub(super) fn handle_reasoning_seed(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_template", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_template dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(super) fn handle_reasoning_pipeline(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_pipeline", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_pipeline dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(super) fn handle_idea_branch_merge(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp = crate::handlers::dispatch_handler(server, "merge", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "merge dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}
