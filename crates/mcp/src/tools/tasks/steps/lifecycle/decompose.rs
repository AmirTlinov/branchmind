#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_decompose(&mut self, args: Value) -> Value {
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
        let parent = match optional_step_path(args_obj, "parent") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
        let Some(steps_array) = steps_value.as_array() else {
            return ai_error("INVALID_INPUT", "steps must be an array");
        };
        if steps_array.is_empty() {
            return ai_error("INVALID_INPUT", "steps must not be empty");
        }

        let mut steps = Vec::with_capacity(steps_array.len());
        for step_value in steps_array {
            let Some(step_obj) = step_value.as_object() else {
                return ai_error("INVALID_INPUT", "steps[] items must be objects");
            };

            let title = match require_string(step_obj, "title") {
                Ok(v) => v,
                Err(resp) => return resp,
            };

            let criteria_value = step_obj
                .get("success_criteria")
                .cloned()
                .unwrap_or(Value::Null);
            let Some(criteria_array) = criteria_value.as_array() else {
                return ai_error("INVALID_INPUT", "steps[].success_criteria must be an array");
            };
            if criteria_array.is_empty() {
                return ai_error(
                    "INVALID_INPUT",
                    "steps[].success_criteria must not be empty",
                );
            }
            let mut success_criteria = Vec::with_capacity(criteria_array.len());
            for item in criteria_array {
                let Some(s) = item.as_str() else {
                    return ai_error(
                        "INVALID_INPUT",
                        "steps[].success_criteria items must be strings",
                    );
                };
                success_criteria.push(s.to_string());
            }
            let success_criteria = match normalize_required_string_list(
                success_criteria,
                "steps[].success_criteria",
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

            steps.push(bm_storage::NewStep {
                title,
                success_criteria,
            });
        }

        let result = self.store.steps_decompose(
            &workspace,
            &task_id,
            expected_revision,
            parent.as_ref(),
            steps,
        );

        match result {
            Ok(out) => {
                let steps_out = out
                    .steps
                    .into_iter()
                    .map(|s| json!({ "step_id": s.step_id, "path": s.path }))
                    .collect::<Vec<_>>();

                ai_ok(
                    "decompose",
                    json!({
                        "task": task_id,
                        "revision": out.task_revision,
                        "steps": steps_out,
                        "event": {
                            "event_id": out.event.event_id(),
                            "ts": ts_ms_to_rfc3339(out.event.ts_ms),
                            "ts_ms": out.event.ts_ms,
                            "task_id": out.event.task_id,
                            "path": out.event.path,
                            "type": out.event.event_type,
                            "payload": parse_json_or_string(&out.event.payload_json)
                        }
                    }),
                )
            }
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
