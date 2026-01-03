#![forbid(unsafe_code)]

mod apply;

use crate::*;
use serde_json::{Value, json};

pub(super) fn handle_task_detail_patch(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: String,
    expected_revision: Option<i64>,
    ops: &[Value],
) -> Value {
    let kind = match parse_plan_or_task_kind(&task_id) {
        Some(v) => v,
        None => return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-"),
    };

    let apply::AppliedTaskDetailOps { patch, fields } =
        match apply::apply_task_detail_ops(server, workspace, kind, &task_id, ops) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    if fields.is_empty() {
        return ai_error("INVALID_INPUT", "no fields to patch");
    }

    let payload = json!({
        "task": task_id,
        "kind": kind.as_str(),
        "fields": fields
    })
    .to_string();

    let result = server.store.task_detail_patch(
        workspace,
        bm_storage::TaskDetailPatchRequest {
            task_id: task_id.clone(),
            expected_revision,
            kind,
            patch,
            event_type: "task_patched".to_string(),
            event_payload_json: payload,
            record_undo: true,
        },
    );

    match result {
        Ok((revision, event)) => ai_ok(
            "patch",
            json!({
                "id": task_id,
                "kind": kind.as_str(),
                "revision": revision,
                "event": {
                    "event_id": event.event_id(),
                    "ts": ts_ms_to_rfc3339(event.ts_ms),
                    "ts_ms": event.ts_ms,
                    "task_id": event.task_id,
                    "path": event.path,
                    "type": event.event_type,
                    "payload": parse_json_or_string(&event.payload_json)
                }
            }),
        ),
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
