#![forbid(unsafe_code)]

use super::super::super::{OpsHistoryTarget, StepRef, StoreError};
use super::parse::{
    snapshot_optional_i64, snapshot_optional_string, snapshot_required_bool, snapshot_required_str,
    snapshot_required_vec,
};
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

pub(super) fn apply_step_patch_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let success_criteria = snapshot_required_vec(snapshot, "success_criteria")?;
    let tests = snapshot_required_vec(snapshot, "tests")?;
    let blockers = snapshot_required_vec(snapshot, "blockers")?;
    let criteria_confirmed = snapshot_required_bool(snapshot, "criteria_confirmed")?;
    let tests_confirmed = snapshot_required_bool(snapshot, "tests_confirmed")?;
    let security_confirmed = snapshot_required_bool(snapshot, "security_confirmed")?;
    let perf_confirmed = snapshot_required_bool(snapshot, "perf_confirmed")?;
    let docs_confirmed = snapshot_required_bool(snapshot, "docs_confirmed")?;
    let completed = snapshot_required_bool(snapshot, "completed")?;
    let completed_at_ms = snapshot_optional_i64(snapshot, "completed_at_ms")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let block_reason = snapshot_optional_string(snapshot, "block_reason")?;

    super::super::super::bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    if completed {
        if let Some(completed_at_ms) = completed_at_ms {
            let changed = tx.execute(
                r#"
                UPDATE steps
                SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                    perf_confirmed=?8, docs_confirmed=?9, completed=?10, completed_at_ms=?11,
                    blocked=?12, block_reason=?13, updated_at_ms=?14
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    if criteria_confirmed { 1i64 } else { 0i64 },
                    if tests_confirmed { 1i64 } else { 0i64 },
                    if security_confirmed { 1i64 } else { 0i64 },
                    if perf_confirmed { 1i64 } else { 0i64 },
                    if docs_confirmed { 1i64 } else { 0i64 },
                    1i64,
                    completed_at_ms,
                    if blocked { 1i64 } else { 0i64 },
                    if blocked { block_reason.clone() } else { None },
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::StepNotFound);
            }
        } else {
            let changed = tx.execute(
                r#"
                UPDATE steps
                SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                    perf_confirmed=?8, docs_confirmed=?9, completed=?10,
                    blocked=?11, block_reason=?12, updated_at_ms=?13
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    if criteria_confirmed { 1i64 } else { 0i64 },
                    if tests_confirmed { 1i64 } else { 0i64 },
                    if security_confirmed { 1i64 } else { 0i64 },
                    if perf_confirmed { 1i64 } else { 0i64 },
                    if docs_confirmed { 1i64 } else { 0i64 },
                    1i64,
                    if blocked { 1i64 } else { 0i64 },
                    if blocked { block_reason.clone() } else { None },
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::StepNotFound);
            }
        }
    } else {
        let changed = tx.execute(
            r#"
            UPDATE steps
            SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                perf_confirmed=?8, docs_confirmed=?9, completed=?10, completed_at_ms=NULL,
                blocked=?11, block_reason=?12, updated_at_ms=?13
            WHERE workspace=?1 AND task_id=?2 AND step_id=?3
            "#,
            params![
                workspace.as_str(),
                task_id,
                step_id,
                title,
                if criteria_confirmed { 1i64 } else { 0i64 },
                if tests_confirmed { 1i64 } else { 0i64 },
                if security_confirmed { 1i64 } else { 0i64 },
                if perf_confirmed { 1i64 } else { 0i64 },
                if docs_confirmed { 1i64 } else { 0i64 },
                0i64,
                if blocked { 1i64 } else { 0i64 },
                if blocked { block_reason.clone() } else { None },
                now_ms
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    }

    tx.execute(
        "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in success_criteria.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }
    tx.execute(
        "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in tests.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }
    tx.execute(
        "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in blockers.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}
