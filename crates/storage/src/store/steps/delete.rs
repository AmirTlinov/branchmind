#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn step_delete(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        record_undo: bool,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let step_ids = collect_step_subtree_ids_tx(&tx, workspace.as_str(), task_id, &step_id)?;

        for step_id in step_ids.iter() {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_notes WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;

            let node_ids = {
                let mut stmt = tx.prepare(
                    "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                )?;
                let rows = stmt
                    .query_map(params![workspace.as_str(), task_id, step_id], |row| {
                        row.get::<_, String>(0)
                    })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for node_id in node_ids {
                tx.execute(
                    "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
                    params![workspace.as_str(), node_id],
                )?;
            }
            tx.execute(
                "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                params![workspace.as_str(), task_id, step_id],
            )?;
        }

        for step_id in step_ids.iter() {
            tx.execute(
                "DELETE FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_deleted_payload(task_id, &step_ref);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_deleted",
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

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id),
                    path: Some(path.clone()),
                    intent: "step_delete",
                    payload_json: &event_payload_json,
                    before_json: None,
                    after_json: None,
                    undoable: false,
                    now_ms,
                },
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn task_root_delete(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        record_undo: bool,
    ) -> Result<(TaskKind, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let kind = if id.starts_with("PLAN-") {
            TaskKind::Plan
        } else if id.starts_with("TASK-") {
            TaskKind::Task
        } else {
            return Err(StoreError::InvalidInput(
                "task must start with PLAN- or TASK-",
            ));
        };

        if matches!(kind, TaskKind::Plan) {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::UnknownId);
            }
            let task_ids = {
                let mut stmt =
                    tx.prepare("SELECT id FROM tasks WHERE workspace=?1 AND parent_plan_id=?2")?;
                let rows = stmt.query_map(params![workspace.as_str(), id], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for task_id in task_ids.iter() {
                delete_task_rows_tx(&tx, workspace.as_str(), task_id)?;
                let event_payload_json = build_task_deleted_payload(task_id, TaskKind::Task);
                let task_event = insert_event_tx(
                    &tx,
                    workspace.as_str(),
                    now_ms,
                    Some(task_id.to_string()),
                    None,
                    "task_deleted",
                    &event_payload_json,
                )?;
                let reasoning_ref =
                    ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
                let _ = ingest_task_event_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.trace_doc,
                    &task_event,
                )?;
            }

            tx.execute(
                "DELETE FROM plan_checklist WHERE workspace=?1 AND plan_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM plans WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id],
            )?;
        } else {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::UnknownId);
            }
            delete_task_rows_tx(&tx, workspace.as_str(), id)?;
        }

        let event_payload_json = build_task_deleted_payload(id, kind);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            "task_deleted",
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(id),
                    path: None,
                    intent: "task_delete",
                    payload_json: &event_payload_json,
                    before_json: None,
                    after_json: None,
                    undoable: false,
                    now_ms,
                },
            )?;
        }

        tx.commit()?;
        Ok((kind, event))
    }
}
