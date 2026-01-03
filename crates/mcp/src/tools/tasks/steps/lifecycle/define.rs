#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_define(&mut self, args: Value) -> Value {
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

        let (step_id, path) = match super::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let title = match optional_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let success_criteria = match optional_string_array(args_obj, "success_criteria") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let tests = match optional_string_array(args_obj, "tests") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };
        let blockers = match optional_string_array(args_obj, "blockers") {
            Ok(v) => normalize_optional_string_list(v),
            Err(resp) => return resp,
        };

        let result = self.store.step_define(
            &workspace,
            bm_storage::StepDefineRequest {
                task_id: task_id.clone(),
                expected_revision,
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                patch: bm_storage::StepPatch {
                    title,
                    success_criteria,
                    tests,
                    blockers,
                    proof_tests_mode: None,
                    proof_security_mode: None,
                    proof_perf_mode: None,
                    proof_docs_mode: None,
                },
            },
        );

        match result {
            Ok(out) => ai_ok(
                "define",
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
}
