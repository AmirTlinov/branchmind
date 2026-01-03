#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn ensure_reasoning_ref(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        kind: TaskKind,
    ) -> Result<ReasoningRefRow, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let exists = match kind {
            TaskKind::Plan => tx
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
            TaskKind::Task => tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
        };

        if !exists {
            return Err(StoreError::UnknownId);
        }

        if let Some(row) = tx
            .query_row(
                r#"
                SELECT branch, notes_doc, graph_doc, trace_doc
                FROM reasoning_refs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(ReasoningRefRow {
                        branch: row.get(0)?,
                        notes_doc: row.get(1)?,
                        graph_doc: row.get(2)?,
                        trace_doc: row.get(3)?,
                    })
                },
            )
            .optional()?
        {
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                &row.branch,
                &row.notes_doc,
                DocumentKind::Notes.as_str(),
                now_ms,
            )?;
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                &row.branch,
                &row.trace_doc,
                DocumentKind::Trace.as_str(),
                now_ms,
            )?;
            tx.commit()?;
            return Ok(row);
        }

        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let reference = ReasoningRef::for_entity(kind, id);
        tx.execute(
            r#"
            INSERT INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                workspace.as_str(),
                id,
                kind.as_str(),
                reference.branch,
                reference.notes_doc,
                reference.graph_doc,
                reference.trace_doc,
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
            &tx,
            workspace.as_str(),
            &row.branch,
            &row.notes_doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &row.branch,
            &row.trace_doc,
            DocumentKind::Trace.as_str(),
            now_ms,
        )?;

        tx.commit()?;
        Ok(row)
    }

    pub fn reasoning_ref_get(
        &self,
        workspace: &WorkspaceId,
        id: &str,
        kind: TaskKind,
    ) -> Result<Option<ReasoningRefRow>, StoreError> {
        let exists = match kind {
            TaskKind::Plan => self
                .conn
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
            TaskKind::Task => self
                .conn
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
        };

        if !exists {
            return Err(StoreError::UnknownId);
        }

        self.conn
            .query_row(
                r#"
                SELECT branch, notes_doc, graph_doc, trace_doc
                FROM reasoning_refs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(ReasoningRefRow {
                        branch: row.get(0)?,
                        notes_doc: row.get(1)?,
                        graph_doc: row.get(2)?,
                        trace_doc: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}
