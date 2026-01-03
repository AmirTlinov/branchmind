#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn doc_append_note(
        &mut self,
        workspace: &WorkspaceId,
        request: DocAppendRequest,
    ) -> Result<DocEntryRow, StoreError> {
        self.doc_append_entry(workspace, DocumentKind::Notes, DocEntryKind::Note, request)
    }

    pub fn doc_append_trace(
        &mut self,
        workspace: &WorkspaceId,
        request: DocAppendRequest,
    ) -> Result<DocEntryRow, StoreError> {
        self.doc_append_entry(workspace, DocumentKind::Trace, DocEntryKind::Note, request)
    }

    fn doc_append_entry(
        &mut self,
        workspace: &WorkspaceId,
        doc_kind: DocumentKind,
        entry_kind: DocEntryKind,
        request: DocAppendRequest,
    ) -> Result<DocEntryRow, StoreError> {
        let DocAppendRequest {
            branch,
            doc,
            title,
            format,
            meta_json,
            content,
        } = request;

        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if content.trim().is_empty() {
            return Err(StoreError::InvalidInput("content must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &branch,
            &doc,
            doc_kind.as_str(),
            now_ms,
        )?;

        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                workspace.as_str(),
                &branch,
                &doc,
                now_ms,
                entry_kind.as_str(),
                title.as_deref(),
                format.as_deref(),
                meta_json.as_deref(),
                &content
            ],
        )?;
        let seq = tx.last_insert_rowid();
        touch_document_tx(&tx, workspace.as_str(), &branch, &doc, now_ms)?;

        tx.commit()?;
        Ok(DocEntryRow {
            seq,
            ts_ms: now_ms,
            branch,
            doc,
            kind: entry_kind,
            title,
            format,
            meta_json,
            content: Some(content),
            source_event_id: None,
            event_type: None,
            task_id: None,
            path: None,
            payload_json: None,
        })
    }
}
