#![forbid(unsafe_code)]

use super::super::super::{OpsHistoryTarget, StepRef, StoreError};
use super::parse::{snapshot_optional_i64, snapshot_required_bool, snapshot_required_str};
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

pub(super) fn apply_step_progress_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let completed = snapshot_required_bool(snapshot, "completed")?;
    let completed_at_ms = snapshot_optional_i64(snapshot, "completed_at_ms")?;

    super::super::super::bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    if completed {
        let ts = completed_at_ms.unwrap_or(now_ms);
        let changed = tx.execute(
            "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, ts],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    } else {
        let changed = tx.execute(
            "UPDATE steps SET completed=0, completed_at_ms=NULL, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}
