#![forbid(unsafe_code)]

use super::super::super::StoreError;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn task_items_list_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
) -> Result<Vec<String>, StoreError> {
    let mut stmt = tx.prepare(
        r#"
        SELECT text
        FROM task_items
        WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4
        ORDER BY ordinal ASC
        "#,
    )?;
    let rows = stmt.query_map(params![workspace, entity_kind, entity_id, field], |row| {
        row.get::<_, String>(0)
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(in crate::store) fn step_items_list_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    step_id: &str,
    table: &str,
) -> Result<Vec<String>, StoreError> {
    let sql = match table {
        "step_criteria" => {
            "SELECT text FROM step_criteria WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        "step_tests" => {
            "SELECT text FROM step_tests WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        "step_blockers" => {
            "SELECT text FROM step_blockers WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        _ => return Err(StoreError::InvalidInput("unknown step items table")),
    };
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map(params![workspace, step_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(in crate::store) fn task_items_replace_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
    items: &[String],
) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM task_items WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4",
        params![workspace, entity_kind, entity_id, field],
    )?;
    for (idx, text) in items.iter().enumerate() {
        tx.execute(
            "INSERT INTO task_items(workspace, entity_kind, entity_id, field, ordinal, text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![workspace, entity_kind, entity_id, field, idx as i64, text],
        )?;
    }
    Ok(())
}

pub(in crate::store) fn checkpoint_required_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    checkpoint: &str,
) -> Result<bool, StoreError> {
    let has_note = tx
        .query_row(
            "SELECT 1 FROM checkpoint_notes WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND checkpoint=?4 LIMIT 1",
            params![workspace, entity_kind, entity_id, checkpoint],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if has_note {
        return Ok(true);
    }
    let has_evidence = tx
        .query_row(
            "SELECT 1 FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND checkpoint=?4 LIMIT 1",
            params![workspace, entity_kind, entity_id, checkpoint],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(has_evidence)
}

pub(in crate::store) fn checkpoint_proof_exists_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    checkpoint: &str,
) -> Result<bool, StoreError> {
    // Proofs are represented by either:
    // - explicit checkpoint notes (human-authored attestations), or
    // - checkpoint evidence refs (artifacts/checks/attachments linked to a checkpoint).
    //
    // This is intentionally the same discovery rule as "checkpoint becomes required once proof exists",
    // but the name makes the intent explicit at call sites.
    checkpoint_required_tx(tx, workspace, entity_kind, entity_id, checkpoint)
}
