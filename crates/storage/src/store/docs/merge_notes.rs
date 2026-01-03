#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn doc_merge_notes(
        &mut self,
        workspace: &WorkspaceId,
        request: DocMergeNotesRequest,
    ) -> Result<MergeNotesResult, StoreError> {
        let DocMergeNotesRequest {
            from_branch,
            into_branch,
            doc,
            cursor,
            limit,
            dry_run,
        } = request;

        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), &from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), &into_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        if !dry_run {
            ensure_workspace_tx(&tx, workspace, now_ms)?;
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                &into_branch,
                &doc,
                DocumentKind::Notes.as_str(),
                now_ms,
            )?;
        }

        // Merge candidates are entries present in sourceView(from_branch) but not in destView(into_branch).
        let diff = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            &into_branch,
            &from_branch,
            &doc,
            before_seq,
            limit,
        )?;

        let mut merged = 0usize;
        let mut skipped = 0usize;
        let mut touched = false;

        for entry in diff.entries.iter() {
            if entry.kind != DocEntryKind::Note {
                skipped += 1;
                continue;
            }

            let Some(content) = entry.content.as_deref() else {
                skipped += 1;
                continue;
            };

            let merge_key = format!("merge:{from_branch}:{}", entry.seq);
            if dry_run {
                let exists = tx
                    .query_row(
                        "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4 LIMIT 1",
                        params![workspace.as_str(), &into_branch, &doc, &merge_key],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if exists {
                    skipped += 1;
                } else {
                    merged += 1;
                }
                continue;
            }

            let meta_json = merge_meta_json(
                entry.meta_json.as_deref(),
                &from_branch,
                entry.seq,
                entry.ts_ms,
            );

            let inserted = tx.execute(
                r#"
                INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content, source_event_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    workspace.as_str(),
                    &into_branch,
                    &doc,
                    now_ms,
                    DocEntryKind::Note.as_str(),
                    entry.title.as_deref(),
                    entry.format.as_deref(),
                    &meta_json,
                    content,
                    &merge_key
                ],
            )?;

            if inserted > 0 {
                merged += 1;
                touched = true;
            } else {
                skipped += 1;
            }
        }

        if touched {
            touch_document_tx(&tx, workspace.as_str(), &into_branch, &doc, now_ms)?;
        }

        tx.commit()?;
        Ok(MergeNotesResult {
            merged,
            skipped,
            count: diff.entries.len(),
            next_cursor: diff.next_cursor,
            has_more: diff.has_more,
        })
    }
}
