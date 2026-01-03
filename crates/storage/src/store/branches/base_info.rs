use super::super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn branch_base_info(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<Option<(String, i64)>, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        let tx = self.conn.transaction()?;
        let info = branch_base_info_tx(&tx, workspace.as_str(), branch)?;
        tx.commit()?;
        Ok(info)
    }
}
