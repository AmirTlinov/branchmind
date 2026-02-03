#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

use super::{branchmind, tasks};

pub(crate) fn dispatch_handler(server: &mut McpServer, name: &str, args: Value) -> Option<Value> {
    if name == "storage" {
        return Some(server.tool_storage(args));
    }

    if let Some(stripped) = name.strip_prefix("tasks_") {
        return tasks::dispatch_tasks_tool(server, stripped, args);
    }

    branchmind::dispatch_branchmind_tool(server, name, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn handler_definitions_and_dispatch_are_in_sync() {
        let mut defined = BTreeSet::<String>::new();
        for tool in super::super::handler_definitions() {
            let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            defined.insert(name.to_string());
        }

        let mut dispatched = BTreeSet::<String>::new();
        dispatched.insert("storage".to_string());
        for name in branchmind::dispatch_branchmind_tool_names() {
            dispatched.insert((*name).to_string());
        }
        for name in tasks::dispatch_tasks_tool_names() {
            dispatched.insert(format!("tasks_{name}"));
        }

        let missing_in_definitions = dispatched.difference(&defined).cloned().collect::<Vec<_>>();
        let missing_in_dispatch = defined.difference(&dispatched).cloned().collect::<Vec<_>>();

        assert!(
            missing_in_definitions.is_empty() && missing_in_dispatch.is_empty(),
            "tool dispatch/definitions mismatch\n  dispatch-only: {missing_in_definitions:?}\n  definitions-only: {missing_in_dispatch:?}"
        );
    }
}
