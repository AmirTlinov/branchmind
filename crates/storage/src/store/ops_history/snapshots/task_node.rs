#![forbid(unsafe_code)]

use super::super::super::{OpsHistoryTarget, StoreError};
use super::parse::{
    snapshot_optional_string, snapshot_required_bool, snapshot_required_str, snapshot_required_vec,
};
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

pub(super) fn apply_task_node_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let node_id = snapshot_required_str(snapshot, "node_id")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let status = snapshot_required_str(snapshot, "status")?;
    let status_manual = snapshot_required_bool(snapshot, "status_manual")?;
    let priority = snapshot_required_str(snapshot, "priority")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let description = snapshot_optional_string(snapshot, "description")?;
    let context = snapshot_optional_string(snapshot, "context")?;
    let blockers = snapshot_required_vec(snapshot, "blockers")?;
    let dependencies = snapshot_required_vec(snapshot, "dependencies")?;
    let next_steps = snapshot_required_vec(snapshot, "next_steps")?;
    let problems = snapshot_required_vec(snapshot, "problems")?;
    let risks = snapshot_required_vec(snapshot, "risks")?;
    let success_criteria = snapshot_required_vec(snapshot, "success_criteria")?;

    super::super::super::bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    let changed = tx.execute(
        r#"
        UPDATE task_nodes
        SET title=?4, status=?5, status_manual=?6, priority=?7, blocked=?8,
            description=?9, context=?10, updated_at_ms=?11
        WHERE workspace=?1 AND task_id=?2 AND node_id=?3
        "#,
        params![
            workspace.as_str(),
            task_id,
            node_id,
            title,
            status,
            if status_manual { 1i64 } else { 0i64 },
            priority,
            if blocked { 1i64 } else { 0i64 },
            description,
            context,
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(StoreError::UnknownId);
    }

    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "blockers",
        &blockers,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "dependencies",
        &dependencies,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "next_steps",
        &next_steps,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "problems",
        &problems,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "risks",
        &risks,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "success_criteria",
        &success_criteria,
    )?;

    Ok(OpsHistoryTarget::TaskNode)
}
