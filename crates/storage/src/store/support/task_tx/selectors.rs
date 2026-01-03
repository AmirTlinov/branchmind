#![forbid(unsafe_code)]

use super::super::super::StoreError;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn resolve_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    path: &StepPath,
) -> Result<String, StoreError> {
    let mut parent_step_id: Option<String> = None;
    for ordinal in path.indices() {
        let step_id: Option<String> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL AND ordinal=?3",
                    params![workspace, task_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3 AND ordinal=?4",
                    params![workspace, task_id, parent_step_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
        };

        let Some(step_id) = step_id else {
            return Err(StoreError::StepNotFound);
        };
        parent_step_id = Some(step_id);
    }
    parent_step_id.ok_or(StoreError::StepNotFound)
}

pub(in crate::store) fn step_path_for_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
) -> Result<String, StoreError> {
    let mut ordinals = Vec::new();
    let mut current = step_id.to_string();

    for _ in 0..128 {
        let row = tx
            .query_row(
                "SELECT parent_step_id, ordinal FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace, task_id, current],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((parent, ordinal)) = row else {
            return Err(StoreError::StepNotFound);
        };
        ordinals.push(ordinal as usize);
        match parent {
            None => break,
            Some(parent_id) => current = parent_id,
        }
    }

    ordinals.reverse();
    Ok(ordinals
        .into_iter()
        .map(|i| format!("s:{i}"))
        .collect::<Vec<_>>()
        .join("."))
}

pub(in crate::store) fn resolve_step_selector_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: Option<&str>,
    path: Option<&StepPath>,
) -> Result<(String, String), StoreError> {
    match (step_id, path) {
        (Some(step_id), _) => {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                    params![workspace, task_id, step_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::StepNotFound);
            }
            Ok((
                step_id.to_string(),
                step_path_for_step_id_tx(tx, workspace, task_id, step_id)?,
            ))
        }
        (None, Some(path)) => {
            let step_id = resolve_step_id_tx(tx, workspace, task_id, path)?;
            Ok((step_id, path.to_string()))
        }
        (None, None) => Err(StoreError::InvalidInput("step selector is required")),
    }
}

pub(in crate::store) fn task_node_path_for_parent_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    parent_step_id: &str,
    ordinal: i64,
) -> Result<String, StoreError> {
    let step_path = step_path_for_step_id_tx(tx, workspace, task_id, parent_step_id)?;
    Ok(format!("{step_path}.t:{ordinal}"))
}

pub(in crate::store) fn resolve_task_node_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    parent_step_id: &str,
    ordinal: i64,
) -> Result<String, StoreError> {
    tx.query_row(
        "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3 AND ordinal=?4",
        params![workspace, task_id, parent_step_id, ordinal],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or(StoreError::UnknownId)
}

pub(in crate::store) fn resolve_task_node_selector_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    node_id: Option<&str>,
    parent_path: Option<&StepPath>,
    ordinal: Option<i64>,
) -> Result<(String, String, String, i64), StoreError> {
    match (node_id, parent_path, ordinal) {
        (Some(node_id), _, _) => {
            let row = tx
                .query_row(
                    "SELECT parent_step_id, ordinal FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND node_id=?3",
                    params![workspace, task_id, node_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((parent_step_id, ordinal)) = row else {
                return Err(StoreError::UnknownId);
            };
            let path =
                task_node_path_for_parent_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            Ok((node_id.to_string(), path, parent_step_id, ordinal))
        }
        (None, Some(parent_path), Some(ordinal)) => {
            let parent_step_id = resolve_step_id_tx(tx, workspace, task_id, parent_path)?;
            let node_id =
                resolve_task_node_id_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            let path =
                task_node_path_for_parent_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            Ok((node_id, path, parent_step_id, ordinal))
        }
        _ => Err(StoreError::InvalidInput("task node selector is required")),
    }
}
