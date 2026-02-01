#![forbid(unsafe_code)]

use crate::McpServer;
use crate::ops::{ToolName, handle_ops_call};
use serde_json::Value;

pub(crate) fn handle(server: &mut McpServer, args: Value) -> Value {
    handle_ops_call(server, ToolName::DocsOps, args)
}
