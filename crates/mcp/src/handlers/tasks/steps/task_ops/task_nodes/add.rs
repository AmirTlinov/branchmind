#![forbid(unsafe_code)]

use super::render::task_node_op_result_json;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_task_add(&mut self, args: Value) -> Value {
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
        let parent_raw = match require_string(args_obj, "parent_step") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_path = match StepPath::parse(&parent_raw) {
            Ok(v) => v,
            Err(_) => return ai_error("INVALID_INPUT", "parent_step is invalid"),
        };
        let title = match require_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = args_obj
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("TODO")
            .to_string();
        let status_manual = args_obj
            .get("status_manual")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let priority = args_obj
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("MEDIUM")
            .to_string();
        let blocked = args_obj
            .get("blocked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let description = args_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let context = args_obj
            .get("context")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());

        let blockers = match optional_string_array(args_obj, "blockers") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let dependencies = match optional_string_array(args_obj, "dependencies") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let next_steps = match optional_string_array(args_obj, "next_steps") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let problems = match optional_string_array(args_obj, "problems") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let risks = match optional_string_array(args_obj, "risks") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let success_criteria = match optional_string_array(args_obj, "success_criteria") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };

        let items = bm_storage::TaskNodeItems {
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        };

        let result = self.store.task_node_add(
            &workspace,
            bm_storage::TaskNodeAddRequest {
                task_id: task_id.clone(),
                expected_revision,
                parent_path: parent_path.clone(),
                title,
                status,
                status_manual,
                priority,
                blocked,
                description,
                context,
                items,
                record_undo: true,
            },
        );

        match result {
            Ok(out) => ai_ok("task_add", task_node_op_result_json(task_id, out)),
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
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Parent step not found"),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
