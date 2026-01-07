#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::paths::StepPath;

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
}
