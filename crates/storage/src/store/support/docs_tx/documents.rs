use super::super::super::StoreError;
use rusqlite::{Transaction, params};

pub(in crate::store) fn ensure_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    kind: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO documents(workspace, branch, doc, kind, created_at_ms, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(workspace, branch, doc) DO NOTHING
        "#,
        params![workspace, branch, doc, kind, now_ms, now_ms],
    )?;
    Ok(())
}

pub(in crate::store) fn touch_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE documents SET updated_at_ms=?4 WHERE workspace=?1 AND branch=?2 AND doc=?3",
        params![workspace, branch, doc, now_ms],
    )?;
    Ok(())
}
