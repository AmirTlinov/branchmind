#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn task_node_delete(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskNodeDeleteRequest,
    ) -> Result<TaskNodeOpResult, StoreError> {
        let TaskNodeDeleteRequest {
            task_id,
            expected_revision,
            selector,
            record_undo,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), &task_id, expected_revision, now_ms)?;
        let (node_id, path, _parent_step_id, _ordinal) = resolve_task_node_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.node_id.as_deref(),
            selector.parent_path.as_ref(),
            selector.ordinal,
        )?;

        tx.execute(
            "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
            params![workspace.as_str(), node_id],
        )?;
        tx.execute(
            "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND node_id=?3",
            params![workspace.as_str(), task_id, node_id],
        )?;

        let event_payload_json = build_task_node_deleted_payload(&task_id, &node_id, &path);
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "task_node_deleted",
                payload_json: &event_payload_json,
            },
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "task_node_delete",
                    payload_json: &event_payload_json,
                    before_json: None,
                    after_json: None,
                    undoable: false,
                    now_ms,
                },
            )?;
        }

        tx.commit()?;
        Ok(TaskNodeOpResult {
            task_revision,
            node: TaskNodeRef { node_id, path },
            event,
        })
    }
}
