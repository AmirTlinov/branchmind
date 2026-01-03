use super::super::super::{BranchSource, StoreError};
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn branch_exists_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<bool, StoreError> {
    if tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    Ok(false)
}

pub(in crate::store) fn branch_sources_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Vec<BranchSource>, StoreError> {
    use std::collections::HashSet;

    const MAX_DEPTH: usize = 32;

    let mut sources = Vec::new();
    sources.push(BranchSource {
        branch: branch.to_string(),
        cutoff_seq: None,
    });

    let mut seen = HashSet::new();
    seen.insert(branch.to_string());

    let mut current = branch.to_string();
    let mut inherited_cutoff: Option<i64> = None;

    for depth in 0..MAX_DEPTH {
        let row = tx
            .query_row(
                "SELECT base_branch, base_seq FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace, current],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((base_branch, base_seq)) = row else {
            break;
        };

        if base_branch == current {
            break;
        }

        if seen.contains(&base_branch) {
            return Err(StoreError::BranchCycle);
        }

        let effective = match inherited_cutoff {
            None => base_seq,
            Some(prev) => std::cmp::min(prev, base_seq),
        };

        sources.push(BranchSource {
            branch: base_branch.clone(),
            cutoff_seq: Some(effective),
        });

        seen.insert(base_branch.clone());
        current = base_branch;
        inherited_cutoff = Some(effective);

        if depth == MAX_DEPTH - 1 {
            return Err(StoreError::BranchDepthExceeded);
        }
    }

    Ok(sources)
}

pub(in crate::store) fn branch_base_info_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT base_branch, base_seq FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace, branch],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?)
}

pub(in crate::store) fn base_sources_for_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Vec<BranchSource>, StoreError> {
    let sources = branch_sources_tx(tx, workspace, branch)?;
    Ok(sources.into_iter().skip(1).collect())
}
