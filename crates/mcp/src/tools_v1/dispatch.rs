#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

use super::{
    docs_ops, graph_ops, jobs_ops, open, status, system_ops, tasks_ops, think_ops, vcs_ops,
    workspace_ops,
};

pub(crate) fn dispatch_tool(server: &mut McpServer, name: &str, args: Value) -> Option<Value> {
    let resp = match name {
        "status" => status::handle(server, args),
        "open" => open::handle(server, args),
        "workspace" => workspace_ops::handle(server, args),
        "tasks" => tasks_ops::handle(server, args),
        "jobs" => jobs_ops::handle(server, args),
        "think" => think_ops::handle(server, args),
        "graph" => graph_ops::handle(server, args),
        "vcs" => vcs_ops::handle(server, args),
        "docs" => docs_ops::handle(server, args),
        "system" => system_ops::handle(server, args),
        _ => return None,
    };
    Some(resp)
}
