#![forbid(unsafe_code)]

use super::super::super::StoreError;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn bump_task_revision_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    expected_revision: Option<i64>,
    now_ms: i64,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM tasks WHERE workspace=?1 AND id=?2",
            params![workspace, task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::UnknownId)?;

    if let Some(expected) = expected_revision
        && expected != current
    {
        return Err(StoreError::RevisionMismatch {
            expected,
            actual: current,
        });
    }

    let next = current + 1;
    tx.execute(
        "UPDATE tasks SET revision=?3, updated_at_ms=?4 WHERE workspace=?1 AND id=?2",
        params![workspace, task_id, next, now_ms],
    )?;
    Ok(next)
}

pub(in crate::store) fn bump_plan_revision_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    plan_id: &str,
    expected_revision: Option<i64>,
    now_ms: i64,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM plans WHERE workspace=?1 AND id=?2",
            params![workspace, plan_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::UnknownId)?;

    if let Some(expected) = expected_revision
        && expected != current
    {
        return Err(StoreError::RevisionMismatch {
            expected,
            actual: current,
        });
    }

    let next = current + 1;
    tx.execute(
        "UPDATE plans SET revision=?3, updated_at_ms=?4 WHERE workspace=?1 AND id=?2",
        params![workspace, plan_id, next, now_ms],
    )?;
    Ok(next)
}
