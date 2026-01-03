#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_complete(&mut self, args: Value) -> Value {
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
        let status = args_obj
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("DONE")
            .to_string();

        let payload = json!({ "status": status });

        if task_id.starts_with("PLAN-") {
            let result = self.store.set_plan_status(
                &workspace,
                bm_storage::SetPlanStatusRequest {
                    id: task_id.clone(),
                    expected_revision,
                    status: status.clone(),
                    status_manual: true,
                    event_type: "plan_updated".to_string(),
                    event_payload_json: payload.to_string(),
                },
            );
            return match result {
                Ok((revision, _event)) => {
                    let plan = match self.store.get_plan(&workspace, &task_id) {
                        Ok(Some(p)) => p,
                        Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    ai_ok(
                        "complete",
                        json!( {
                            "plan": {
                                "id": plan.id,
                                "kind": "plan",
                                "title": plan.title,
                                "revision": revision,
                                "criteria_confirmed": plan.criteria_confirmed,
                                "tests_confirmed": plan.tests_confirmed,
                                "criteria_auto_confirmed": plan.criteria_auto_confirmed,
                                "tests_auto_confirmed": plan.tests_auto_confirmed,
                                "security_confirmed": plan.security_confirmed,
                                "perf_confirmed": plan.perf_confirmed,
                                "docs_confirmed": plan.docs_confirmed
                            }
                        }),
                    )
                }
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
                Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown plan id"),
                Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
                Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
            };
        }

        if !task_id.starts_with("TASK-") {
            return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-");
        }

        let require_steps_completed = status == "DONE";
        let result = self.store.set_task_status(
            &workspace,
            bm_storage::SetTaskStatusRequest {
                id: task_id.clone(),
                expected_revision,
                status: status.clone(),
                status_manual: true,
                require_steps_completed,
                event_type: "task_completed".to_string(),
                event_payload_json: payload.to_string(),
            },
        );

        match result {
            Ok((revision, _event)) => {
                let task = match self.store.get_task(&workspace, &task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                ai_ok(
                    "complete",
                    json!( {
                        "task": {
                            "id": task.id,
                            "kind": "task",
                            "title": task.title,
                            "revision": revision,
                            "status": task.status,
                            "criteria_confirmed": task.criteria_confirmed,
                            "tests_confirmed": task.tests_confirmed,
                            "criteria_auto_confirmed": task.criteria_auto_confirmed,
                            "tests_auto_confirmed": task.tests_auto_confirmed,
                            "security_confirmed": task.security_confirmed,
                            "perf_confirmed": task.perf_confirmed,
                            "docs_confirmed": task.docs_confirmed
                        }
                    }),
                )
            }
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
