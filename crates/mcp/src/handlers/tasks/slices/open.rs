#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_slice_open(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if let Err(resp) =
            check_unknown_args(args_obj, &["workspace", "slice_id"], "tasks.slice.open")
        {
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

        let steps = match self
            .store
            .list_task_steps(&workspace, &binding.slice_task_id, None, 200)
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| {
                    json!({
                        "step_id": row.step_id,
                        "path": row.path,
                        "title": row.title,
                        "completed": row.completed,
                        "criteria_confirmed": row.criteria_confirmed,
                        "tests_confirmed": row.tests_confirmed,
                        "blocked": row.blocked,
                        "block_reason": row.block_reason
                    })
                })
                .collect::<Vec<_>>(),
            Err(StoreError::StepNotFound) => Vec::new(),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let actions = slice_actions_for_open(
            workspace.as_str(),
            &binding.plan_id,
            &binding.slice_id,
            &spec.objective,
        );
        ai_ok(
            "tasks_slice_open",
            json!({
                "workspace": workspace.as_str(),
                "slice": {
                    "plan_id": binding.plan_id,
                    "slice_id": binding.slice_id,
                    "slice_task_id": binding.slice_task_id,
                    "title": binding.title,
                    "objective": binding.objective,
                    "status": binding.status,
                    "budgets_json": binding.budgets_json
                },
                "slice_task": {
                    "id": slice_task.id,
                    "title": slice_task.title,
                    "revision": slice_task.revision,
                    "status": slice_task.status,
                    "updated_at_ms": slice_task.updated_at_ms
                },
                "slice_plan_spec": spec.to_json(),
                "steps": steps,
                "actions": actions
            }),
        )
    }
}
