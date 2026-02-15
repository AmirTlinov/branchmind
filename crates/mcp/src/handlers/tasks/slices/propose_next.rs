#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_slices_propose_next(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if let Err(resp) = check_unknown_args(
            args_obj,
            &[
                "workspace",
                "plan",
                "task",
                "objective",
                "constraints",
                "policy",
            ],
            "tasks.slices.propose_next",
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
        let (plan_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj)
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !matches!(kind, TaskKind::Plan) {
            return ai_error("INVALID_INPUT", "plan is required");
        }

        let objective = match optional_string(args_obj, "objective") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let constraints = match optional_string_array(args_obj, "constraints") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return resp,
        };
        let policy = match optional_string(args_obj, "policy") {
            Ok(v) => v.unwrap_or_else(|| "fail_closed".to_string()),
            Err(resp) => return resp,
        };
        if !policy.eq_ignore_ascii_case("fail_closed") {
            return ai_error("INVALID_INPUT", "policy must be fail_closed");
        }

        let plan = match self.store.get_plan(&workspace, &plan_id) {
            Ok(Some(p)) => p,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let spec = crate::support::propose_next_slice_spec(
            &plan_id,
            &plan.title,
            &objective,
            &constraints,
        );
        let actions = vec![action_call(
            "tasks.slices.apply",
            "Create this slice (materialize SliceTasks → Steps).",
            "high",
            json!({
                "workspace": workspace.as_str(),
                "plan": plan_id,
                "expected_revision": plan.revision,
                "policy": "fail_closed",
                "slice_plan_spec": spec.to_json()
            }),
        )];
        ai_ok(
            "tasks_slices_propose_next",
            json!({
                "workspace": workspace.as_str(),
                "plan": { "id": plan_id, "title": plan.title, "revision": plan.revision },
                "policy": "fail_closed",
                "slice_plan_spec": spec.to_json(),
                "actions": actions
            }),
        )
    }
}
