#![forbid(unsafe_code)]

use super::super::super::{OpsHistoryTarget, StepRef, StoreError};
use super::parse::{snapshot_optional_string, snapshot_required_bool, snapshot_required_str};
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

pub(super) fn apply_step_block_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let block_reason = snapshot_optional_string(snapshot, "block_reason")?;

    super::super::super::bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    let changed = tx.execute(
        "UPDATE steps SET blocked=?4, block_reason=?5, updated_at_ms=?6 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
        params![
            workspace.as_str(),
            task_id,
            step_id,
            if blocked { 1i64 } else { 0i64 },
            if blocked { block_reason.clone() } else { None },
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(StoreError::StepNotFound);
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}
