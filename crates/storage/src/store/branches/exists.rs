use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn branch_exists(&self, workspace: &WorkspaceId, branch: &str) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        Ok(false)
    }
}
