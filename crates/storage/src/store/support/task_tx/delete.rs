#![forbid(unsafe_code)]

use super::super::super::StoreError;
use rusqlite::{Transaction, params};

pub(in crate::store) fn collect_step_subtree_ids_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    root_step_id: &str,
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    let mut stack = vec![root_step_id.to_string()];
    while let Some(current) = stack.pop() {
        out.push(current.clone());
        let mut stmt = tx.prepare(
            "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
        )?;
        let rows = stmt.query_map(params![workspace, task_id, current], |row| {
            row.get::<_, String>(0)
        })?;
        for step_id in rows {
            stack.push(step_id?);
        }
    }
    Ok(out)
}

pub(in crate::store) fn collect_task_step_ids_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
) -> Result<Vec<String>, StoreError> {
    let mut stmt = tx.prepare("SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2")?;
    let rows = stmt.query_map(params![workspace, task_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(in crate::store) fn delete_task_rows_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
) -> Result<(), StoreError> {
    let step_ids = collect_task_step_ids_tx(tx, workspace, task_id)?;

    for step_id in step_ids.iter() {
        tx.execute(
            "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_notes WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;

        let node_ids = {
            let mut stmt = tx.prepare(
                "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            )?;
            let rows = stmt.query_map(params![workspace, task_id, step_id], |row| {
                row.get::<_, String>(0)
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for node_id in node_ids {
            tx.execute(
                "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
                params![workspace, node_id],
            )?;
        }
        tx.execute(
            "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            params![workspace, task_id, step_id],
        )?;
    }

    tx.execute(
        "DELETE FROM steps WHERE workspace=?1 AND task_id=?2",
        params![workspace, task_id],
    )?;

    let node_ids = {
        let mut stmt =
            tx.prepare("SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2")?;
        let rows = stmt.query_map(params![workspace, task_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for node_id in node_ids {
        tx.execute(
            "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
            params![workspace, node_id],
        )?;
    }
    tx.execute(
        "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2",
        params![workspace, task_id],
    )?;

    tx.execute(
        "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;

    tx.execute(
        "DELETE FROM tasks WHERE workspace=?1 AND id=?2",
        params![workspace, task_id],
    )?;

    Ok(())
}
