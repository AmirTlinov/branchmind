#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn anchor_aliases_list_for_anchor_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    anchor_id: &str,
) -> Result<Vec<String>, StoreError> {
    let mut stmt = tx.prepare(
        r#"
        SELECT alias_id
        FROM anchor_aliases
        WHERE workspace=?1 AND anchor_id=?2
        ORDER BY alias_id ASC
        "#,
    )?;
    let mut rows = stmt.query(params![workspace, anchor_id])?;
    let mut out = Vec::<String>::new();
    while let Some(row) = rows.next()? {
        out.push(row.get(0)?);
    }
    Ok(out)
}

impl SqliteStore {
    /// Resolve an anchor id, treating `anchor_aliases` as a backwards-compatible redirect table.
    ///
    /// Returns:
    /// - `Some(<canonical_id>)` when either the id exists as an anchor, or it matches an alias.
    /// - `None` when neither anchor nor alias exists.
    pub fn anchor_resolve_id(
        &mut self,
        workspace: &WorkspaceId,
        raw_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let id = crate::store::anchors::normalize_anchor_id(raw_id)?;
        let tx = self.conn.transaction()?;

        let exists = tx
            .query_row(
                "SELECT 1 FROM anchors WHERE workspace=?1 AND id=?2 LIMIT 1",
                params![workspace.as_str(), id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            tx.commit()?;
            return Ok(Some(id));
        }

        let owner: Option<String> = tx
            .query_row(
                "SELECT anchor_id FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
                params![workspace.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        tx.commit()?;
        Ok(owner)
    }
}
