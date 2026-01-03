#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use rusqlite::params;

impl SqliteStore {
    pub fn step_note(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        note: String,
    ) -> Result<StepOpResult, StoreError> {
        if note.trim().is_empty() {
            return Err(StoreError::InvalidInput("note must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        tx.execute(
            "INSERT INTO step_notes(workspace, task_id, step_id, ts_ms, note) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workspace.as_str(), task_id, step_id, now_ms, &note],
        )?;
        let note_seq = tx.last_insert_rowid();

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_noted_payload(task_id, &step_ref, note_seq);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_noted",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        // Mirror the human-authored note content into the reasoning notes document (single organism invariant).
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;
        let meta_json =
            build_step_noted_mirror_meta_json(task_id, &step_ref, note_seq, &event.event_id());
        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.notes_doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                meta_json,
                &note
            ],
        )?;
        touch_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            now_ms,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }
}
