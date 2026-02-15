#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_runner_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    runner_id: String,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    let now_ms = crate::support::now_ms_i64();
    let lease = match server
        .store
        .runner_lease_get(workspace, bm_storage::RunnerLeaseGetRequest { runner_id })
    {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(lease) = lease else {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown runner id",
            Some(
                "Copy a runner:<id> ref from tasks_jobs_radar runner lines or ensure the runner is heartbeating.",
            ),
            vec![],
        ));
    };

    let lease_active = lease.lease.lease_expires_at_ms > now_ms;
    let effective_status = if lease_active {
        lease.lease.status.clone()
    } else {
        "offline".to_string()
    };
    let expires_in_ms = lease
        .lease
        .lease_expires_at_ms
        .saturating_sub(now_ms)
        .max(0);

    if let Some(job_id) = lease.lease.active_job_id.as_deref() {
        suggestions.push(json!({
            "tool": "open",
            "reason": "Open the active job for this runner",
            "args_hint": {
                "workspace": workspace.as_str(),
                "id": job_id
            }
        }));
    }

    Ok(json!({
        "workspace": workspace.as_str(),
        "kind": "runner",
        "id": format!("runner:{}", lease.lease.runner_id),
        "status": effective_status,
        "lease": {
            "runner_id": lease.lease.runner_id,
            "status": lease.lease.status,
            "active_job_id": lease.lease.active_job_id,
            "lease_expires_at_ms": lease.lease.lease_expires_at_ms,
            "created_at_ms": lease.lease.created_at_ms,
            "updated_at_ms": lease.lease.updated_at_ms,
            "lease_active": lease_active,
            "expires_in_ms": expires_in_ms
        },
        "meta": lease
            .meta_json
            .as_ref()
            .map(|raw| parse_json_or_string(raw))
            .unwrap_or(Value::Null),
        "truncated": false
    }))
}
