#![forbid(unsafe_code)]

use super::render::task_node_op_result_json;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_task_define(&mut self, args: Value) -> Value {
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

        let title = match optional_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = args_obj
            .get("status")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let status_manual = args_obj.get("status_manual").and_then(|v| v.as_bool());
        let priority = args_obj
            .get("priority")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let blocked = args_obj.get("blocked").and_then(|v| v.as_bool());
        let description = match optional_nullable_string(args_obj, "description") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let context = match optional_nullable_string(args_obj, "context") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let blockers = match optional_string_array(args_obj, "blockers") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let dependencies = match optional_string_array(args_obj, "dependencies") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let next_steps = match optional_string_array(args_obj, "next_steps") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let problems = match optional_string_array(args_obj, "problems") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let risks = match optional_string_array(args_obj, "risks") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let success_criteria = match optional_string_array(args_obj, "success_criteria") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };

        let patch = bm_storage::TaskNodePatch {
            title,
            status,
            status_manual,
            priority,
            blocked,
            description,
            context,
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        };

        let result = self.store.task_node_patch(
            &workspace,
            bm_storage::TaskNodePatchRequest {
                task_id: task_id.clone(),
                expected_revision,
                selector: bm_storage::TaskNodeSelector {
                    node_id: None,
                    parent_path: Some(parent_path.clone()),
                    ordinal: Some(ordinal),
                },
                patch,
                record_undo: true,
            },
        );

        match result {
            Ok(out) => ai_ok("task_define", task_node_op_result_json(task_id, out)),
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
