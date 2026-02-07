#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn step_resolve(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        step_id: Option<&str>,
        path: Option<&StepPath>,
    ) -> Result<StepRef, StoreError> {
        let tx = self.conn.transaction()?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
        tx.commit()?;
        Ok(StepRef { step_id, path })
    }

    /// Locate a step by its stable `STEP-*` id.
    ///
    /// Returns the owning task id and the step ref (id + path).
    pub fn step_locate(
        &mut self,
        workspace: &WorkspaceId,
        step_id: &str,
    ) -> Result<Option<(String, StepRef)>, StoreError> {
        let step_id = step_id.trim();
        if step_id.is_empty() {
            return Err(StoreError::InvalidInput("step_id must not be empty"));
        }

        let tx = self.conn.transaction()?;
        let task_id = tx
            .query_row(
                "SELECT task_id FROM steps WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(task_id) = task_id else {
            tx.commit()?;
            return Ok(None);
        };

        let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id.as_str(), step_id)?;
        tx.commit()?;
        Ok(Some((
            task_id.clone(),
            StepRef {
                step_id: step_id.to_string(),
                path,
            },
        )))
    }
}
