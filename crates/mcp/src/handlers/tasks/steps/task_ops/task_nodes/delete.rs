#![forbid(unsafe_code)]

use super::render::task_node_op_result_json;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_task_delete(&mut self, args: Value) -> Value {
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
        let path_raw = match require_string(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let (parent_path, ordinal) = match parse_task_node_path(&path_raw) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = self.store.task_node_delete(
            &workspace,
            bm_storage::TaskNodeDeleteRequest {
                task_id: task_id.clone(),
                expected_revision,
                selector: bm_storage::TaskNodeSelector {
                    node_id: None,
                    parent_path: Some(parent_path.clone()),
                    ordinal: Some(ordinal),
                },
                record_undo: true,
            },
        );

        match result {
            Ok(out) => ai_ok("task_delete", task_node_op_result_json(task_id, out)),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Task node not found"),
            Err(StoreError::RevisionMismatch { expected, actual }) => ai_error_with(
                "REVISION_MISMATCH",
                &format!("expected={expected} actual={actual}"),
                Some("Refresh the current revision and retry with expected_revision."),
                vec![suggest_call(
                    "tasks_context",
                    "Refresh current revisions for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
