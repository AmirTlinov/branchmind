#![forbid(unsafe_code)]

use super::super::super::{StepRef, StoreError};
use super::super::json_escape;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn task_graph_node_id(task_id: &str) -> String {
    format!("task:{task_id}")
}

pub(in crate::store) fn step_graph_node_id(step_id: &str) -> String {
    format!("step:{step_id}")
}

pub(in crate::store) fn build_task_graph_meta_json(task_id: &str) -> String {
    format!(
        "{{\"source\":\"tasks\",\"task_id\":\"{}\"}}",
        json_escape(task_id)
    )
}

pub(in crate::store) fn build_step_graph_meta_json(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"source\":\"tasks\",\"task_id\":\"{}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        json_escape(task_id),
        json_escape(&step.step_id),
        json_escape(&step.path)
    )
}

pub(in crate::store) fn task_title_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
) -> Result<String, StoreError> {
    tx.query_row(
        "SELECT title FROM tasks WHERE workspace=?1 AND id=?2",
        params![workspace, task_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or(StoreError::UnknownId)
}

pub(in crate::store) fn step_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
) -> Result<(String, bool), StoreError> {
    let row = tx
        .query_row(
            "SELECT title, completed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace, task_id, step_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((title, completed)) = row else {
        return Err(StoreError::StepNotFound);
    };
    Ok((title, completed != 0))
}
