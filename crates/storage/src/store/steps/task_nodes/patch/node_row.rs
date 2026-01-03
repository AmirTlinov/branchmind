#![forbid(unsafe_code)]

use super::super::super::super::*;
use rusqlite::{OptionalExtension, Transaction, params};

#[derive(Clone, Debug)]
pub(super) struct TaskNodeRowFields {
    pub(super) title: String,
    pub(super) status: String,
    pub(super) status_manual: bool,
    pub(super) priority: String,
    pub(super) blocked: bool,
    pub(super) description: Option<String>,
    pub(super) context: Option<String>,
}

pub(super) struct ScalarPatch {
    pub(super) title: Option<String>,
    pub(super) status: Option<String>,
    pub(super) status_manual: Option<bool>,
    pub(super) priority: Option<String>,
    pub(super) blocked: Option<bool>,
    pub(super) description: Option<Option<String>>,
    pub(super) context: Option<Option<String>>,
}

impl ScalarPatch {
    pub(super) fn apply(self, mut current: TaskNodeRowFields) -> TaskNodeRowFields {
        if let Some(value) = self.title {
            current.title = value;
        }
        if let Some(value) = self.status {
            current.status = value;
        }
        if let Some(value) = self.status_manual {
            current.status_manual = value;
        }
        if let Some(value) = self.priority {
            current.priority = value;
        }
        if let Some(value) = self.blocked {
            current.blocked = value;
        }
        if let Some(value) = self.description {
            current.description = value;
        }
        if let Some(value) = self.context {
            current.context = value;
        }
        current
    }
}

pub(super) fn load_task_node_row_fields_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    node_id: &str,
) -> Result<TaskNodeRowFields, StoreError> {
    let row = tx
        .query_row(
            r#"
            SELECT title, status, status_manual, priority, blocked, description, context
            FROM task_nodes
            WHERE workspace=?1 AND task_id=?2 AND node_id=?3
            "#,
            params![workspace, task_id, node_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;

    let Some((title, status, status_manual, priority, blocked, description, context)) = row else {
        return Err(StoreError::UnknownId);
    };

    Ok(TaskNodeRowFields {
        title,
        status,
        status_manual: status_manual != 0,
        priority,
        blocked: blocked != 0,
        description,
        context,
    })
}

pub(super) fn update_task_node_row_fields_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    node_id: &str,
    next: &TaskNodeRowFields,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        UPDATE task_nodes
        SET title=?4, status=?5, status_manual=?6, priority=?7, blocked=?8,
            description=?9, context=?10, updated_at_ms=?11
        WHERE workspace=?1 AND task_id=?2 AND node_id=?3
        "#,
        params![
            workspace,
            task_id,
            node_id,
            next.title,
            next.status,
            if next.status_manual { 1i64 } else { 0i64 },
            next.priority,
            if next.blocked { 1i64 } else { 0i64 },
            next.description,
            next.context,
            now_ms
        ],
    )?;
    Ok(())
}
