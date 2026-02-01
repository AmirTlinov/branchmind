#![forbid(unsafe_code)]

use crate::ops::{CommandSpec, Envelope, OpError, OpResponse};

/// Dispatch a custom (non-legacy) command.
///
/// Custom commands return `OpResponse` directly (v1 shape), so they can emit `actions[]`
/// without passing through legacy suggestion mapping.
pub(crate) fn dispatch_custom(
    server: &mut crate::McpServer,
    spec: &CommandSpec,
    env: &Envelope,
) -> OpResponse {
    let Some(handler) = spec.handler else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("cmd {} has no handler (and no legacy_tool)", spec.cmd),
                recovery: Some("Check registry wiring for this cmd.".to_string()),
            },
        );
    };
    handler(server, env)
}
