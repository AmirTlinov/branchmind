#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn doc_head_seq_for_branch_doc(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
    ) -> Result<Option<i64>, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }
        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
        let head_seq = doc_head_seq_for_sources_tx(&tx, workspace.as_str(), doc, &sources)?;
        tx.commit()?;
        Ok(head_seq)
    }

    pub fn doc_entry_visible(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        seq: i64,
    ) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        let visible = doc_entry_visible_tx(&tx, workspace.as_str(), branch, doc, seq)?;
        tx.commit()?;
        Ok(visible)
    }
}
