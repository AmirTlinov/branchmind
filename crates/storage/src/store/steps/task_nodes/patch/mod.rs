#![forbid(unsafe_code)]

mod items;
mod node_row;
mod presence;
mod snapshots;

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;

use self::items::ItemsPatch;
use self::node_row::ScalarPatch;
use self::presence::PatchPresence;

impl SqliteStore {
    pub fn task_node_patch(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskNodePatchRequest,
    ) -> Result<TaskNodeOpResult, StoreError> {
        let TaskNodePatchRequest {
            task_id,
            expected_revision,
            selector,
            patch,
            record_undo,
        } = request;

        let TaskNodePatch {
            title,
            status,
            status_manual,
            priority,
            blocked,
            description,
            context,
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        } = patch;

        let scalar_patch = ScalarPatch {
            title,
            status,
            status_manual,
            priority,
            blocked,
            description,
            context,
        };
        let items_patch = ItemsPatch {
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        };
        let presence = PatchPresence::from_parts(&scalar_patch, &items_patch);
        if !presence.any() {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

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

        let current_fields =
            node_row::load_task_node_row_fields_tx(&tx, workspace.as_str(), &task_id, &node_id)?;
        let before_items = items::load_task_node_items_tx(&tx, workspace.as_str(), &node_id)?;
        let before_snapshot = snapshots::task_node_snapshot_json(
            &task_id,
            &node_id,
            &path,
            &current_fields,
            &before_items,
        );

        let next_fields = scalar_patch.apply(current_fields);
        node_row::update_task_node_row_fields_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            &node_id,
            &next_fields,
            now_ms,
        )?;

        let next_items = items_patch.apply(&before_items);
        items::replace_task_node_items_tx(
            &tx,
            workspace.as_str(),
            &node_id,
            &presence,
            &next_items,
        )?;

        let fields = presence.changed_fields();
        let event_payload_json =
            build_task_node_defined_payload(&task_id, &node_id, &path, &fields);
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "task_node_defined",
                payload_json: &event_payload_json,
            },
        )?;

        if record_undo {
            let after_snapshot = snapshots::task_node_snapshot_json(
                task_id.as_str(),
                &node_id,
                &path,
                &next_fields,
                &next_items,
            );
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "task_node_patch",
                    payload_json: &event_payload_json,
                    before_json: Some(&before_snapshot.to_string()),
                    after_json: Some(&after_snapshot.to_string()),
                    undoable: true,
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
