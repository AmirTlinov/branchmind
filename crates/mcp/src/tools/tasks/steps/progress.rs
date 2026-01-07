#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_block(&mut self, args: Value) -> Value {
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
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
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
        let blocked = args_obj
            .get("blocked")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let reason = match optional_string(args_obj, "reason") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let reason_out = reason.clone();

        let result = self.store.step_block_set(
            &workspace,
            bm_storage::StepBlockSetRequest {
                task_id: task_id.clone(),
                expected_revision,
                agent_id: agent_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                blocked,
                reason,
                record_undo: true,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "block",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "blocked": blocked,
                    "reason": reason_out,
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
            Err(StoreError::StepLeaseHeld {
                step_id: leased_step_id,
                holder_agent_id,
                now_seq,
                expires_seq,
            }) => ai_error_with(
                "STEP_LEASE_HELD",
                &format!(
                    "step is leased by {holder_agent_id} (step_id={leased_step_id}, now_seq={now_seq}, expires_seq={expires_seq})"
                ),
                Some("Ask the holder to release, wait for expiry, or take over explicitly."),
                super::lease::lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    agent_id.as_deref(),
                ),
            ),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_progress(&mut self, args: Value) -> Value {
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
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
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
        let completed = match args_obj.get("completed") {
            Some(v) => match v.as_bool() {
                Some(value) => value,
                None => return ai_error("INVALID_INPUT", "completed must be a boolean"),
            },
            None => true,
        };
        let force = args_obj
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let result = self.store.step_progress(
            &workspace,
            bm_storage::StepProgressRequest {
                task_id: task_id.clone(),
                expected_revision,
                agent_id: agent_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                completed,
                force,
                record_undo: true,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "progress",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "completed": completed,
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
                        "checkpoints": super::lifecycle::checkpoints_suggestion_value(criteria, tests, security, perf, docs)
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
                        "checkpoint": super::lifecycle::checkpoints_suggestion_array_value(tests, security, perf, docs),
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
            Err(StoreError::StepLeaseHeld {
                step_id: leased_step_id,
                holder_agent_id,
                now_seq,
                expires_seq,
            }) => ai_error_with(
                "STEP_LEASE_HELD",
                &format!(
                    "step is leased by {holder_agent_id} (step_id={leased_step_id}, now_seq={now_seq}, expires_seq={expires_seq})"
                ),
                Some("Ask the holder to release, wait for expiry, or take over explicitly."),
                super::lease::lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    agent_id.as_deref(),
                ),
            ),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
