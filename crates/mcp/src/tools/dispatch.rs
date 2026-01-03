#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

use super::{branchmind, tasks};

pub(crate) fn dispatch_tool(server: &mut McpServer, name: &str, args: Value) -> Option<Value> {
    if name == "storage" {
        return Some(server.tool_storage(args));
    }

    if let Some(stripped) = name.strip_prefix("tasks_") {
        return tasks::dispatch_tasks_tool(server, stripped, args);
    }

    branchmind::dispatch_branchmind_tool(server, name, args)
}
