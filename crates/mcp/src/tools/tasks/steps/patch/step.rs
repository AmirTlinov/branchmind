#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn handle_step_patch(
    server: &mut McpServer,
    args_obj: &serde_json::Map<String, Value>,
    workspace: &WorkspaceId,
    task_id: String,
    expected_revision: Option<i64>,
    ops: &[Value],
) -> Value {
    let step_id = match optional_string(args_obj, "step_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let path = match optional_step_path(args_obj, "path") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if step_id.is_none() && path.is_none() {
        return ai_error("INVALID_INPUT", "step_id or path is required");
    }

    let detail =
        match server
            .store
            .step_detail(workspace, &task_id, step_id.as_deref(), path.as_ref())
        {
            Ok(v) => v,
            Err(StoreError::StepNotFound) => return ai_error("UNKNOWN_ID", "Step not found"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

    let mut patch = bm_storage::StepPatch {
        title: None,
        success_criteria: None,
        tests: None,
        blockers: None,
        proof_tests_mode: None,
        proof_security_mode: None,
        proof_perf_mode: None,
        proof_docs_mode: None,
    };
    let mut fields: Vec<&str> = Vec::new();
    let mut criteria_list = detail.success_criteria.clone();
    let mut tests_list = detail.tests.clone();
    let mut blockers_list = detail.blockers.clone();

    for op_value in ops {
        let Some(op_obj) = op_value.as_object() else {
            return ai_error("INVALID_INPUT", "ops entries must be objects");
        };
        let op_name = match require_string(op_obj, "op") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let field = match require_string(op_obj, "field") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let value = op_obj.get("value");

        match field.as_str() {
            "title" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "title supports only set");
                }
                let Some(Value::String(v)) = value else {
                    return ai_error("INVALID_INPUT", "title must be a string");
                };
                patch.title = Some(v.clone());
                fields.push("title");
            }
            "success_criteria" => {
                if let Err(resp) =
                    apply_list_op(&mut criteria_list, &op_name, value, "success_criteria")
                {
                    return resp;
                }
                patch.success_criteria = Some(criteria_list.clone());
                fields.push("success_criteria");
            }
            "tests" => {
                if let Err(resp) = apply_list_op(&mut tests_list, &op_name, value, "tests") {
                    return resp;
                }
                patch.tests = Some(tests_list.clone());
                fields.push("tests");
            }
            "blockers" => {
                if let Err(resp) = apply_list_op(&mut blockers_list, &op_name, value, "blockers") {
                    return resp;
                }
                patch.blockers = Some(blockers_list.clone());
                fields.push("blockers");
            }
            _ => return ai_error("INVALID_INPUT", "unknown step field"),
        }
    }

    if fields.is_empty() {
        return ai_error("INVALID_INPUT", "no fields to patch");
    }

    let payload = json!({
        "task": task_id,
        "step_id": detail.step_id,
        "path": detail.path,
        "fields": fields
    })
    .to_string();

    let result = server.store.step_patch(
        workspace,
        bm_storage::StepPatchRequest {
            task_id: task_id.clone(),
            expected_revision,
            selector: bm_storage::StepSelector {
                step_id: Some(detail.step_id.clone()),
                path: None,
            },
            patch,
            event_payload_json: payload,
            record_undo: true,
        },
    );

    match result {
        Ok(out) => ai_ok(
            "patch",
            json!({
                "task": task_id,
                "revision": out.task_revision,
                "step": { "step_id": out.step.step_id, "path": out.step.path },
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
        ),
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
