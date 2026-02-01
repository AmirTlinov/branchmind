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
        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "step_deleted",
                payload_json: &event_payload_json,
            },
        )?;

        let mut graph_touched = false;
        for step_id in step_ids.iter() {
            let node_id = step_graph_node_id(step_id);
            graph_touched |= Self::project_task_graph_delete_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &node_id,
                now_ms,
            )?;
        }
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

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
        let (event_payload_json, event) = if matches!(kind, TaskKind::Plan) {
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
                let step_ids = collect_task_step_ids_tx(&tx, workspace.as_str(), task_id)?;
                delete_task_rows_tx(&tx, workspace.as_str(), task_id)?;
                let event_payload_json = build_task_deleted_payload(task_id, TaskKind::Task);
                let (task_event, reasoning_ref) = emit_task_event_tx(
                    &tx,
                    TaskEventEmitTxArgs {
                        workspace,
                        now_ms,
                        task_id,
                        kind: TaskKind::Task,
                        path: None,
                        event_type: "task_deleted",
                        payload_json: &event_payload_json,
                    },
                )?;
                let mut graph_touched = false;
                for step_id in step_ids.iter() {
                    let node_id = step_graph_node_id(step_id);
                    graph_touched |= Self::project_task_graph_delete_node_tx(
                        &tx,
                        workspace.as_str(),
                        &reasoning_ref,
                        &task_event,
                        &node_id,
                        now_ms,
                    )?;
                }
                let task_node_id = task_graph_node_id(task_id);
                graph_touched |= Self::project_task_graph_delete_node_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref,
                    &task_event,
                    &task_node_id,
                    now_ms,
                )?;
                if graph_touched {
                    touch_document_tx(
                        &tx,
                        workspace.as_str(),
                        &reasoning_ref.branch,
                        &reasoning_ref.graph_doc,
                        now_ms,
                    )?;
                }
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

            let payload = build_task_deleted_payload(id, kind);
            let (plan_event, _reasoning_ref) = emit_task_event_tx(
                &tx,
                TaskEventEmitTxArgs {
                    workspace,
                    now_ms,
                    task_id: id,
                    kind,
                    path: None,
                    event_type: "task_deleted",
                    payload_json: &payload,
                },
            )?;
            (payload, plan_event)
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
            let step_ids = collect_task_step_ids_tx(&tx, workspace.as_str(), id)?;
            delete_task_rows_tx(&tx, workspace.as_str(), id)?;
            let payload = build_task_deleted_payload(id, kind);
            let (task_event, reasoning_ref) = emit_task_event_tx(
                &tx,
                TaskEventEmitTxArgs {
                    workspace,
                    now_ms,
                    task_id: id,
                    kind,
                    path: None,
                    event_type: "task_deleted",
                    payload_json: &payload,
                },
            )?;
            let mut graph_touched = false;
            for step_id in step_ids.iter() {
                let node_id = step_graph_node_id(step_id);
                graph_touched |= Self::project_task_graph_delete_node_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref,
                    &task_event,
                    &node_id,
                    now_ms,
                )?;
            }
            let task_node_id = task_graph_node_id(id);
            graph_touched |= Self::project_task_graph_delete_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &task_event,
                &task_node_id,
                now_ms,
            )?;
            if graph_touched {
                touch_document_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.graph_doc,
                    now_ms,
                )?;
            }
            (payload, task_event)
        };

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
