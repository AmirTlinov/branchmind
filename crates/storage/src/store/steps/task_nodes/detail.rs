#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn task_node_detail(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        node_id: Option<&str>,
        parent_path: Option<&StepPath>,
        ordinal: Option<i64>,
    ) -> Result<TaskNodeDetail, StoreError> {
        let tx = self.conn.transaction()?;
        let (node_id, path, _parent_step_id, _ordinal) = resolve_task_node_selector_tx(
            &tx,
            workspace.as_str(),
            task_id,
            node_id,
            parent_path,
            ordinal,
        )?;
        let row = tx
            .query_row(
                r#"
                SELECT node_id, task_id, parent_step_id, ordinal, title, status, status_manual,
                       priority, blocked, description, context, created_at_ms, updated_at_ms
                FROM task_nodes
                WHERE workspace=?1 AND task_id=?2 AND node_id=?3
                "#,
                params![workspace.as_str(), task_id, node_id],
                |row| {
                    Ok(TaskNodeRow {
                        node_id: row.get(0)?,
                        task_id: row.get(1)?,
                        parent_step_id: row.get(2)?,
                        ordinal: row.get(3)?,
                        title: row.get(4)?,
                        status: row.get(5)?,
                        status_manual: row.get::<_, i64>(6)? != 0,
                        priority: row.get(7)?,
                        blocked: row.get::<_, i64>(8)? != 0,
                        description: row.get(9)?,
                        context: row.get(10)?,
                        created_at_ms: row.get(11)?,
                        updated_at_ms: row.get(12)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Err(StoreError::UnknownId);
        };

        let blockers = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "blockers",
        )?;
        let dependencies = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "dependencies",
        )?;
        let next_steps = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "next_steps",
        )?;
        let problems = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "problems",
        )?;
        let risks =
            task_items_list_tx(&tx, workspace.as_str(), "task_node", &row.node_id, "risks")?;
        let success_criteria = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "success_criteria",
        )?;

        tx.commit()?;
        Ok(TaskNodeDetail {
            row,
            path,
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        })
    }
}
