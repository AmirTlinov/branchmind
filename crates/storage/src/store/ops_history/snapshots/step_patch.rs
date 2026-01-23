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
    let next_action = match snapshot.get("next_action") {
        None => None,
        Some(JsonValue::Null) => Some(None),
        Some(JsonValue::String(value)) => Some(Some(value.clone())),
        _ => return Err(StoreError::InvalidInput("snapshot invalid string field")),
    };
    let stop_criteria = match snapshot.get("stop_criteria") {
        None => None,
        Some(JsonValue::Null) => Some(None),
        Some(JsonValue::String(value)) => Some(Some(value.clone())),
        _ => return Err(StoreError::InvalidInput("snapshot invalid string field")),
    };
    let proof_tests_mode = snapshot_optional_i64(snapshot, "proof_tests_mode")?;
    let proof_security_mode = snapshot_optional_i64(snapshot, "proof_security_mode")?;
    let proof_perf_mode = snapshot_optional_i64(snapshot, "proof_perf_mode")?;
    let proof_docs_mode = snapshot_optional_i64(snapshot, "proof_docs_mode")?;
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

    let (
        current_next_action,
        current_stop_criteria,
        current_proof_tests_mode,
        current_proof_security_mode,
        current_proof_perf_mode,
        current_proof_docs_mode,
    ) = tx.query_row(
        r#"
        SELECT next_action, stop_criteria,
               proof_tests_mode, proof_security_mode, proof_perf_mode, proof_docs_mode
        FROM steps
        WHERE workspace=?1 AND task_id=?2 AND step_id=?3
        "#,
        params![workspace.as_str(), task_id, step_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        },
    )?;

    let next_action = next_action.unwrap_or(current_next_action);
    let stop_criteria = stop_criteria.unwrap_or(current_stop_criteria);
    let proof_tests_mode = proof_tests_mode.unwrap_or(current_proof_tests_mode);
    let proof_security_mode = proof_security_mode.unwrap_or(current_proof_security_mode);
    let proof_perf_mode = proof_perf_mode.unwrap_or(current_proof_perf_mode);
    let proof_docs_mode = proof_docs_mode.unwrap_or(current_proof_docs_mode);

    if completed {
        if let Some(completed_at_ms) = completed_at_ms {
            let changed = tx.execute(
                r#"
                UPDATE steps
                SET title=?4, next_action=?5, stop_criteria=?6,
                    proof_tests_mode=?7, proof_security_mode=?8, proof_perf_mode=?9, proof_docs_mode=?10,
                    criteria_confirmed=?11, tests_confirmed=?12, security_confirmed=?13,
                    perf_confirmed=?14, docs_confirmed=?15, completed=?16, completed_at_ms=?17,
                    blocked=?18, block_reason=?19, updated_at_ms=?20
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    next_action,
                    stop_criteria,
                    proof_tests_mode,
                    proof_security_mode,
                    proof_perf_mode,
                    proof_docs_mode,
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
                SET title=?4, next_action=?5, stop_criteria=?6,
                    proof_tests_mode=?7, proof_security_mode=?8, proof_perf_mode=?9, proof_docs_mode=?10,
                    criteria_confirmed=?11, tests_confirmed=?12, security_confirmed=?13,
                    perf_confirmed=?14, docs_confirmed=?15, completed=?16,
                    blocked=?17, block_reason=?18, updated_at_ms=?19
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    next_action,
                    stop_criteria,
                    proof_tests_mode,
                    proof_security_mode,
                    proof_perf_mode,
                    proof_docs_mode,
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
            SET title=?4, next_action=?5, stop_criteria=?6,
                proof_tests_mode=?7, proof_security_mode=?8, proof_perf_mode=?9, proof_docs_mode=?10,
                criteria_confirmed=?11, tests_confirmed=?12, security_confirmed=?13,
                perf_confirmed=?14, docs_confirmed=?15, completed=?16, completed_at_ms=NULL,
                blocked=?17, block_reason=?18, updated_at_ms=?19
            WHERE workspace=?1 AND task_id=?2 AND step_id=?3
            "#,
            params![
                workspace.as_str(),
                task_id,
                step_id,
                title,
                next_action,
                stop_criteria,
                proof_tests_mode,
                proof_security_mode,
                proof_perf_mode,
                proof_docs_mode,
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
