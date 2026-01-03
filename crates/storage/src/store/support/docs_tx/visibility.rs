use super::super::super::StoreError;
use super::branches::{branch_exists_tx, branch_sources_tx};
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn doc_entry_visible_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    seq: i64,
) -> Result<bool, StoreError> {
    if seq <= 0 {
        return Ok(false);
    }
    if !branch_exists_tx(tx, workspace, branch)? {
        return Err(StoreError::UnknownBranch);
    }

    let row = tx
        .query_row(
            "SELECT branch FROM doc_entries WHERE workspace=?1 AND doc=?2 AND seq=?3",
            params![workspace, doc, seq],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(entry_branch) = row else {
        return Ok(false);
    };

    let sources = branch_sources_tx(tx, workspace, branch)?;
    for source in sources {
        if source.branch == entry_branch {
            if let Some(cutoff) = source.cutoff_seq {
                return Ok(seq <= cutoff);
            }
            return Ok(true);
        }
    }
    Ok(false)
}
