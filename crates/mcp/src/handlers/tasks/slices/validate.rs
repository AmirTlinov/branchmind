#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_slice_validate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if let Err(resp) = check_unknown_args(
            args_obj,
            &["workspace", "slice_id", "policy"],
            "tasks.slice.validate",
        ) {
            return resp;
        }
        if !self.slice_plans_v1_enabled {
            return ai_error_with(
                "FEATURE_DISABLED",
                "slice_plans_v1 is disabled",
                Some("Enable via --slice-plans-v1 (or env BRANCHMIND_SLICE_PLANS_V1=1)."),
                Vec::new(),
            );
        }
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let slice_id = match require_string(args_obj, "slice_id") {
            Ok(v) => v.trim().to_string(),
            Err(resp) => return resp,
        };
        if slice_id.is_empty() {
            return ai_error("INVALID_INPUT", "slice_id must not be empty");
        }
        let policy = match optional_string(args_obj, "policy") {
            Ok(v) => v.unwrap_or_else(|| "fail_closed".to_string()),
            Err(resp) => return resp,
        };
        if !policy.eq_ignore_ascii_case("fail_closed") {
            return ai_error("INVALID_INPUT", "policy must be fail_closed");
        }

        let binding = match self.store.plan_slice_get_by_slice_id(&workspace, &slice_id) {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown slice_id (no plan_slices binding)"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let slice_task = match self.store.get_task(&workspace, &binding.slice_task_id) {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown slice_task_id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let spec = match crate::support::parse_slice_plan_spec_from_task_context(
            slice_task.context.as_deref(),
        ) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "slice_task_id missing slice_plan_spec JSON in context",
                );
            }
            Err(resp) => return resp,
        };

        // Basic structural validation already enforced by parser (3..10, tests, blockers).
        // Additionally, validate that the stored step tree matches the spec exactly (deterministic).
        let steps = match self
            .store
            .list_task_steps(&workspace, &binding.slice_task_id, None, 400)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if let Err(resp) = crate::support::validate_slice_step_tree(&steps, &spec) {
            return resp;
        }
        let total_steps = steps.len();

        ai_ok(
            "tasks_slice_validate",
            json!({
                "workspace": workspace.as_str(),
                "slice_id": slice_id,
                "policy": "fail_closed",
                "status": "pass",
                "binding": {
                    "plan_id": binding.plan_id,
                    "slice_task_id": binding.slice_task_id,
                    "status": binding.status
                },
                "budgets": spec.budgets.to_json(),
                "summary": {
                    "tasks": spec.tasks.len(),
                    "total_steps": total_steps
                }
            }),
        )
    }
}
