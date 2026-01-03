use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn branch_rename(
        &mut self,
        workspace: &WorkspaceId,
        from: &str,
        to: &str,
    ) -> Result<(String, String), StoreError> {
        let from = from.trim();
        let to = to.trim();
        if from.is_empty() {
            return Err(StoreError::InvalidInput("from must not be empty"));
        }
        if to.is_empty() {
            return Err(StoreError::InvalidInput("to must not be empty"));
        }
        if from == to {
            return Err(StoreError::InvalidInput("from and to must differ"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), from)? {
            return Err(StoreError::UnknownBranch);
        }
        if branch_exists_tx(&tx, workspace.as_str(), to)? {
            return Err(StoreError::BranchAlreadyExists);
        }

        tx.execute(
            "UPDATE branches SET name=?1 WHERE workspace=?2 AND name=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE branches SET base_branch=?1 WHERE workspace=?2 AND base_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE branch_checkout SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE reasoning_refs SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE documents SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE doc_entries SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_node_versions SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_edge_versions SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_conflicts SET from_branch=?1 WHERE workspace=?2 AND from_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_conflicts SET into_branch=?1 WHERE workspace=?2 AND into_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_refs SET ref=?1 WHERE workspace=?2 AND ref=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_refs SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_reflog SET ref=?1 WHERE workspace=?2 AND ref=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_reflog SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_tags SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;

        tx.commit()?;
        Ok((from.to_string(), to.to_string()))
    }
}
