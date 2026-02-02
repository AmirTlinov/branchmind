#![forbid(unsafe_code)]

use crate::store::StoreError;
use rusqlite::{OptionalExtension, Transaction, params};

use super::super::types::GraphConflictSignatureArgs;

#[derive(Clone, Debug)]
pub(in crate::store) struct GraphConflictStatusBySignatureRow {
    pub(in crate::store) conflict_id: String,
    pub(in crate::store) status: String,
    pub(in crate::store) created_at_ms: i64,
    pub(in crate::store) resolved_at_ms: Option<i64>,
}

pub(in crate::store) fn graph_conflict_status_row_by_signature_tx(
    tx: &Transaction<'_>,
    args: GraphConflictSignatureArgs<'_>,
) -> Result<Option<GraphConflictStatusBySignatureRow>, StoreError> {
    let GraphConflictSignatureArgs {
        workspace,
        from_branch,
        into_branch,
        doc,
        kind,
        key,
        base_cutoff_seq,
        theirs_seq,
    } = args;

    // IMPORTANT: signature intentionally excludes ours_seq.
    //
    // This makes conflict identity stable across merges where "ours" keeps changing (e.g. when
    // applying `use_from`), so resolved/open conflicts do not "zombie-resurface" with new ids.
    Ok(tx
        .query_row(
            r#"
            SELECT conflict_id, status, created_at_ms, resolved_at_ms
            FROM graph_conflicts
            WHERE
              workspace=?1
              AND from_branch=?2
              AND into_branch=?3
              AND doc=?4
              AND kind=?5
              AND key=?6
              AND base_cutoff_seq=?7
              AND theirs_seq=?8
            ORDER BY
              CASE status
                WHEN 'resolved' THEN 2
                WHEN 'open' THEN 1
                ELSE 0
              END DESC,
              resolved_at_ms DESC,
              created_at_ms DESC,
              conflict_id DESC
            LIMIT 1
            "#,
            params![
                workspace,
                from_branch,
                into_branch,
                doc,
                kind,
                key,
                base_cutoff_seq,
                theirs_seq
            ],
            |row| {
                Ok(GraphConflictStatusBySignatureRow {
                    conflict_id: row.get(0)?,
                    status: row.get(1)?,
                    created_at_ms: row.get(2)?,
                    resolved_at_ms: row.get(3)?,
                })
            },
        )
        .optional()?)
}
