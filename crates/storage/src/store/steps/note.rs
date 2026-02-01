#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn step_note(
        &mut self,
        workspace: &WorkspaceId,
        request: StepNoteRequest,
    ) -> Result<StepOpResult, StoreError> {
        if request.note.trim().is_empty() {
            return Err(StoreError::InvalidInput("note must not be empty"));
        }
        let task_id = request.task_id;
        let note = request.note;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision = bump_task_revision_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            request.expected_revision,
            now_ms,
        )?;
        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            request.selector.step_id.as_deref(),
            request.selector.path.as_ref(),
        )?;
        super::lease::enforce_step_lease_tx(
            &tx,
            workspace.as_str(),
            &step_id,
            request.agent_id.as_deref(),
        )?;

        tx.execute(
            "INSERT INTO step_notes(workspace, task_id, step_id, ts_ms, note) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workspace.as_str(), &task_id, step_id, now_ms, &note],
        )?;
        let note_seq = tx.last_insert_rowid();

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_noted_payload(&task_id, &step_ref, note_seq);
        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "step_noted",
                payload_json: &event_payload_json,
            },
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
            build_step_noted_mirror_meta_json(&task_id, &step_ref, note_seq, &event.event_id());
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
