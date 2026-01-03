use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn branch_delete(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<bool, StoreError> {
        let branch = name.trim();
        if branch.is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        if let Some(current) = branch_checkout_get_tx(&tx, workspace.as_str())?
            && current == branch
        {
            return Err(StoreError::InvalidInput(
                "cannot delete the currently checked-out branch",
            ));
        }

        let has_children = tx
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND base_branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_row| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if has_children {
            return Err(StoreError::InvalidInput(
                "branch has dependent branches; delete or rebase them first",
            ));
        }

        let referenced = tx
            .query_row(
                "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_row| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if referenced {
            return Err(StoreError::InvalidInput(
                "branch is referenced by reasoning refs; move them before deletion",
            ));
        }

        tx.execute(
            "DELETE FROM vcs_refs WHERE workspace=?1 AND ref=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM vcs_reflog WHERE workspace=?1 AND ref=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM vcs_tags WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_conflicts WHERE workspace=?1 AND (from_branch=?2 OR into_branch=?2)",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_edge_versions WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_node_versions WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM doc_entries WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM documents WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        let deleted = tx.execute(
            "DELETE FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace.as_str(), branch],
        )?;

        tx.commit()?;
        Ok(deleted > 0)
    }
}
