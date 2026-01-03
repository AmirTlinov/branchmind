#![forbid(unsafe_code)]

use super::super::*;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

pub(super) fn load_step_detail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
    path: &str,
) -> Result<(StepDetail, Option<i64>), StoreError> {
    let row = tx
        .query_row(
            r#"
            SELECT title, criteria_confirmed, tests_confirmed, security_confirmed,
                   perf_confirmed, docs_confirmed, completed, completed_at_ms, blocked, block_reason
            FROM steps
            WHERE workspace=?1 AND task_id=?2 AND step_id=?3
            "#,
            params![workspace, task_id, step_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;

    let Some((
        title,
        criteria,
        tests,
        security,
        perf,
        docs,
        completed,
        completed_at_ms,
        blocked,
        block_reason,
    )) = row
    else {
        return Err(StoreError::StepNotFound);
    };

    let detail = StepDetail {
        step_id: step_id.to_string(),
        path: path.to_string(),
        title,
        success_criteria: step_items_list_tx(tx, workspace, step_id, "step_criteria")?,
        tests: step_items_list_tx(tx, workspace, step_id, "step_tests")?,
        blockers: step_items_list_tx(tx, workspace, step_id, "step_blockers")?,
        criteria_confirmed: criteria != 0,
        tests_confirmed: tests != 0,
        security_confirmed: security != 0,
        perf_confirmed: perf != 0,
        docs_confirmed: docs != 0,
        completed: completed != 0,
        blocked: blocked != 0,
        block_reason,
    };

    Ok((detail, completed_at_ms))
}

pub(super) fn step_detail_snapshot_json(
    task_id: &str,
    detail: &StepDetail,
    completed_at_ms: Option<i64>,
) -> Value {
    json!({
        "task": task_id,
        "step_id": detail.step_id,
        "path": detail.path,
        "title": detail.title,
        "success_criteria": detail.success_criteria,
        "tests": detail.tests,
        "blockers": detail.blockers,
        "criteria_confirmed": detail.criteria_confirmed,
        "tests_confirmed": detail.tests_confirmed,
        "security_confirmed": detail.security_confirmed,
        "perf_confirmed": detail.perf_confirmed,
        "docs_confirmed": detail.docs_confirmed,
        "completed": detail.completed,
        "completed_at_ms": completed_at_ms,
        "blocked": detail.blocked,
        "block_reason": detail.block_reason
    })
}
