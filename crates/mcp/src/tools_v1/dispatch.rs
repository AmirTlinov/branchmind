#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

use super::{tool_branch, tool_merge, tool_think};

pub(crate) fn dispatch_tool(server: &mut McpServer, name: &str, args: Value) -> Option<Value> {
    let resp = match name {
        "think" => tool_think::handle(server, args),
        "branch" => tool_branch::handle(server, args),
        "merge" => tool_merge::handle(server, args),
        _ => return None,
    };
    Some(resp)
}
