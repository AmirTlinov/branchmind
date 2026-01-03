#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn doc_diff_tail(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        to_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if to_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("to_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), to_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let slice = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            from_branch,
            to_branch,
            doc,
            before_seq,
            limit,
        )?;
        tx.commit()?;
        Ok(slice)
    }
}
