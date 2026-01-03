#![forbid(unsafe_code)]

use super::super::super::{OpsHistoryTarget, StoreError};
use super::parse::{
    snapshot_optional_json_string, snapshot_optional_string, snapshot_required_str,
    snapshot_required_vec,
};
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

pub(super) fn apply_task_detail_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let kind_raw = snapshot_required_str(snapshot, "kind")?;
    let kind = match kind_raw.as_str() {
        "plan" => TaskKind::Plan,
        "task" => TaskKind::Task,
        _ => return Err(StoreError::InvalidInput("snapshot kind invalid")),
    };
    let task_id = snapshot_required_str(snapshot, "task")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let description = snapshot_optional_string(snapshot, "description")?;
    let context = snapshot_optional_string(snapshot, "context")?;
    let priority = snapshot_required_str(snapshot, "priority")?;
    let contract = snapshot_optional_string(snapshot, "contract")?;
    let contract_json = snapshot_optional_json_string(snapshot, "contract_data")?;
    let domain = snapshot_optional_string(snapshot, "domain")?;
    let phase = snapshot_optional_string(snapshot, "phase")?;
    let component = snapshot_optional_string(snapshot, "component")?;
    let assignee = snapshot_optional_string(snapshot, "assignee")?;
    let tags = snapshot_required_vec(snapshot, "tags")?;
    let depends_on = snapshot_required_vec(snapshot, "depends_on")?;

    match kind {
        TaskKind::Plan => {
            super::super::super::bump_plan_revision_tx(
                tx,
                workspace.as_str(),
                &task_id,
                None,
                now_ms,
            )?;
            let changed = tx.execute(
                r#"
                UPDATE plans
                SET title=?3, description=?4, context=?5, priority=?6, contract=?7, contract_json=?8, updated_at_ms=?9
                WHERE workspace=?1 AND id=?2
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    title,
                    description,
                    context,
                    priority,
                    contract,
                    contract_json,
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::UnknownId);
            }
        }
        TaskKind::Task => {
            super::super::super::bump_task_revision_tx(
                tx,
                workspace.as_str(),
                &task_id,
                None,
                now_ms,
            )?;
            let changed = tx.execute(
                r#"
                UPDATE tasks
                SET title=?3, description=?4, context=?5, priority=?6,
                    domain=?7, phase=?8, component=?9, assignee=?10, updated_at_ms=?11
                WHERE workspace=?1 AND id=?2
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    title,
                    description,
                    context,
                    priority,
                    domain,
                    phase,
                    component,
                    assignee,
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::UnknownId);
            }
        }
    }

    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        kind.as_str(),
        &task_id,
        "tags",
        &tags,
    )?;
    super::super::super::task_items_replace_tx(
        tx,
        workspace.as_str(),
        kind.as_str(),
        &task_id,
        "depends_on",
        &depends_on,
    )?;

    Ok(OpsHistoryTarget::Task {
        title: if matches!(kind, TaskKind::Task) {
            Some(title)
        } else {
            None
        },
    })
}
