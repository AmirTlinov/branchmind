#![forbid(unsafe_code)]

use super::super::strict_gate::{StrictGateContext, enforce_strict_reasoning_gate};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_done(&mut self, args: Value) -> Value {
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

        if let Err(resp) = enforce_strict_reasoning_gate(StrictGateContext {
            server: self,
            workspace: &workspace,
            task_id: &task_id,
            step_id: step_id.as_deref(),
            path: path.as_ref(),
            args_obj,
            reasoning_override: None,
            allow_override: false,
            close_args_obj: None,
            warnings: None,
            note_event: None,
        }) {
            return resp;
        }

        let result = self.store.step_done(
            &workspace,
            &task_id,
            expected_revision,
            step_id.as_deref(),
            path.as_ref(),
        );

        match result {
            Ok(out) => ai_ok(
                "done",
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
            Err(StoreError::CheckpointsNotConfirmed {
                criteria,
                tests,
                security,
                perf,
                docs,
            }) => ai_error_with(
                "CHECKPOINTS_NOT_CONFIRMED",
                &format!(
                    "missing: criteria={criteria} tests={tests} security={security} perf={perf} docs={docs}"
                ),
                Some("Confirm missing checkpoints before completing the step."),
                vec![suggest_call(
                    "tasks_verify",
                    "Confirm required checkpoints for this step.",
                    "high",
                    json!({
                        "workspace": workspace.as_str(),
                        "task": task_id,
                        "step_id": step_id,
                        "path": args_obj.get("path").cloned().unwrap_or(Value::Null),
                        "checkpoints": super::checkpoints_suggestion_value(criteria, tests, security, perf, docs)
                    }),
                )],
            ),
            Err(StoreError::ProofMissing {
                tests,
                security,
                perf,
                docs,
            }) => ai_error_with(
                "PROOF_REQUIRED",
                &format!(
                    "missing proof: tests={tests} security={security} perf={perf} docs={docs}"
                ),
                Some(
                    "Attach proof receipts to the required checkpoints (CMD + LINK via tasks_evidence_capture), then retry completing the step.",
                ),
                vec![suggest_call(
                    "tasks_evidence_capture",
                    "Attach proof to the missing checkpoints for this step.",
                    "high",
                    json!({
                        "workspace": workspace.as_str(),
                        "task": task_id,
                        "step_id": step_id,
                        "path": args_obj.get("path").cloned().unwrap_or(Value::Null),
                        "checkpoint": super::checkpoints_suggestion_array_value(tests, security, perf, docs),
                        "checks": proof_checks_placeholder_json()
                    }),
                )],
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
