#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn doc_list(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<Vec<DocumentRow>, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let docs = {
            let mut stmt = tx.prepare(
                "SELECT doc, kind, created_at_ms, updated_at_ms \
                 FROM documents WHERE workspace=?1 AND branch=?2 \
                 ORDER BY updated_at_ms DESC, doc ASC",
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), branch])?;
            let mut docs = Vec::new();

            while let Some(row) = rows.next()? {
                let kind: String = row.get(1)?;
                let kind = match kind.as_str() {
                    "notes" => DocumentKind::Notes,
                    "trace" => DocumentKind::Trace,
                    "graph" => DocumentKind::Graph,
                    _ => DocumentKind::Notes,
                };
                docs.push(DocumentRow {
                    branch: branch.to_string(),
                    doc: row.get(0)?,
                    kind,
                    created_at_ms: row.get(2)?,
                    updated_at_ms: row.get(3)?,
                });
            }

            docs
        };

        tx.commit()?;
        Ok(docs)
    }
}
