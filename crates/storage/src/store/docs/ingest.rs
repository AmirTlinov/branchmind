#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn doc_ingest_task_event(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        event: &EventRow,
    ) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, event.ts_ms)?;
        let inserted = ingest_task_event_tx(&tx, workspace.as_str(), branch, doc, event)?;
        tx.commit()?;
        Ok(inserted)
    }
}
