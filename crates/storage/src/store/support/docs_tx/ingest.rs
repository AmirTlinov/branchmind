use super::super::super::{DocEntryKind, DocumentKind, EventRow, StoreError};
use super::documents::{ensure_document_tx, touch_document_tx};
use rusqlite::{Transaction, params};

pub(in crate::store) fn ingest_task_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    event: &EventRow,
) -> Result<bool, StoreError> {
    ensure_document_tx(
        tx,
        workspace,
        branch,
        doc,
        DocumentKind::Trace.as_str(),
        event.ts_ms,
    )?;

    let event_id = event.event_id();
    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, source_event_id, event_type, task_id, path, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            workspace,
            branch,
            doc,
            event.ts_ms,
            DocEntryKind::Event.as_str(),
            event_id,
            &event.event_type,
            event.task_id.as_deref(),
            event.path.as_deref(),
            &event.payload_json
        ],
    )?;

    if inserted > 0 {
        touch_document_tx(tx, workspace, branch, doc, event.ts_ms)?;
    }

    Ok(inserted > 0)
}
