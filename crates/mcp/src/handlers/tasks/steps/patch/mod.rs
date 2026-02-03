#![forbid(unsafe_code)]
//! `tasks_patch` split by patch kind.

mod step;
mod task_detail;
mod task_node;

use crate::*;
use serde_json::Value;

impl McpServer {
    pub(crate) fn tool_tasks_patch(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, _kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let kind = args_obj
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("task_detail");

        let ops_value = args_obj.get("ops").cloned().unwrap_or(Value::Null);
        let ops = ops_value
            .as_array()
            .ok_or_else(|| ai_error("INVALID_INPUT", "ops must be an array"));
        let Ok(ops) = ops else {
            return ops.err().unwrap();
        };
        if ops.is_empty() {
            return ai_error("INVALID_INPUT", "ops must not be empty");
        }

        match kind {
            "task_detail" => task_detail::handle_task_detail_patch(
                self,
                &workspace,
                task_id,
                expected_revision,
                ops,
            ),
            "step" => {
                step::handle_step_patch(self, args_obj, &workspace, task_id, expected_revision, ops)
            }
            "task" => task_node::handle_task_node_patch(
                self,
                args_obj,
                &workspace,
                task_id,
                expected_revision,
                ops,
            ),
            _ => ai_error("INVALID_INPUT", "kind must be task_detail|step|task"),
        }
    }
}
