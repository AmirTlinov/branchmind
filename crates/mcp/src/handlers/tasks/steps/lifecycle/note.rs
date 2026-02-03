#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_note(&mut self, args: Value) -> Value {
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
        let note = match require_string(args_obj, "note") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (step_id, path) = match super::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // Keep copies for error mapping (suggestions need the original selectors).
        let agent_id_for_errors = agent_id.clone();
        let step_id_for_errors = step_id.clone();
        let path_for_errors = path.clone();

        let result = self.store.step_note(
            &workspace,
            bm_storage::StepNoteRequest {
                task_id: task_id.clone(),
                expected_revision,
                agent_id,
                selector: bm_storage::StepSelector { step_id, path },
                note,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "note",
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
                super::super::lease::lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id_for_errors.as_deref(),
                    path_for_errors.as_ref(),
                    agent_id_for_errors.as_deref(),
                ),
            ),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
