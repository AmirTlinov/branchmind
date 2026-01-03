use super::super::super::{DocumentKind, ReasoningRefRow, StoreError};
use super::documents::ensure_document_tx;
use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use rusqlite::{Transaction, params};

pub(in crate::store) fn ensure_reasoning_ref_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    id: &str,
    kind: TaskKind,
    now_ms: i64,
) -> Result<ReasoningRefRow, StoreError> {
    let reference = ReasoningRef::for_entity(kind, id);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace.as_str(),
            id,
            kind.as_str(),
            &reference.branch,
            &reference.notes_doc,
            &reference.graph_doc,
            &reference.trace_doc,
            now_ms
        ],
    )?;

    let row = ReasoningRefRow {
        branch: reference.branch,
        notes_doc: reference.notes_doc,
        graph_doc: reference.graph_doc,
        trace_doc: reference.trace_doc,
    };

    ensure_document_tx(
        tx,
        workspace.as_str(),
        &row.branch,
        &row.notes_doc,
        DocumentKind::Notes.as_str(),
        now_ms,
    )?;
    ensure_document_tx(
        tx,
        workspace.as_str(),
        &row.branch,
        &row.trace_doc,
        DocumentKind::Trace.as_str(),
        now_ms,
    )?;

    Ok(row)
}
