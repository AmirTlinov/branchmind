use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn branch_create(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
        from: Option<&str>,
    ) -> Result<BranchInfo, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if branch_exists_tx(&tx, workspace.as_str(), name)? {
            return Err(StoreError::BranchAlreadyExists);
        }

        let base_branch = match from {
            Some(v) if !v.trim().is_empty() => v.to_string(),
            Some(_) => return Err(StoreError::InvalidInput("from must not be empty")),
            None => {
                if let Some(branch) = branch_checkout_get_tx(&tx, workspace.as_str())? {
                    branch
                } else {
                    let _ = bootstrap_default_branch_tx(&tx, workspace.as_str(), now_ms)?;
                    if let Some(branch) = branch_checkout_get_tx(&tx, workspace.as_str())? {
                        branch
                    } else if branch_exists_tx(&tx, workspace.as_str(), DEFAULT_BRANCH)? {
                        DEFAULT_BRANCH.to_string()
                    } else {
                        return Err(StoreError::InvalidInput(
                            "from is required when no checkout branch is set",
                        ));
                    }
                }
            }
        };

        if !branch_exists_tx(&tx, workspace.as_str(), &base_branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let base_seq = doc_entries_head_seq_tx(&tx, workspace.as_str())?.unwrap_or(0);

        tx.execute(
            r#"
            INSERT INTO branches(workspace, name, base_branch, base_seq, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                workspace.as_str(),
                name,
                base_branch.as_str(),
                base_seq,
                now_ms
            ],
        )?;

        tx.commit()?;
        Ok(BranchInfo {
            name: name.to_string(),
            base_branch: Some(base_branch),
            base_seq: Some(base_seq),
            created_at_ms: Some(now_ms),
        })
    }
}
