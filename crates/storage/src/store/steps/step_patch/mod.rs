#![forbid(unsafe_code)]

mod detail;

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn step_patch(
        &mut self,
        workspace: &WorkspaceId,
        request: StepPatchRequest,
    ) -> Result<StepOpResult, StoreError> {
        let StepPatchRequest {
            task_id,
            expected_revision,
            agent_id,
            selector,
            patch,
            event_payload_json,
            record_undo,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), &task_id, expected_revision, now_ms)?;
        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.step_id.as_deref(),
            selector.path.as_ref(),
        )?;
        super::lease::enforce_step_lease_tx(
            &tx,
            workspace.as_str(),
            &step_id,
            agent_id.as_deref(),
        )?;

        let (detail, before_completed_at_ms) =
            detail::load_step_detail_tx(&tx, workspace.as_str(), &task_id, &step_id, &path)?;
        let before_snapshot =
            detail::step_detail_snapshot_json(&task_id, &detail, before_completed_at_ms);

        if let Some(title) = patch.title {
            tx.execute(
                "UPDATE steps SET title=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, title, now_ms],
            )?;
        }
        if let Some(items) = patch.success_criteria {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET criteria_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }
        if let Some(items) = patch.tests {
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET tests_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }
        if let Some(items) = patch.blockers {
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
        }

        if let Some(next_action) = patch.next_action {
            tx.execute(
                "UPDATE steps SET next_action=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, next_action, now_ms],
            )?;
        }
        if let Some(stop_criteria) = patch.stop_criteria {
            tx.execute(
                "UPDATE steps SET stop_criteria=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, stop_criteria, now_ms],
            )?;
        }

        if let Some(mode) = patch.proof_tests_mode {
            tx.execute(
                "UPDATE steps SET proof_tests_mode=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    mode.as_i64(),
                    now_ms
                ],
            )?;
        }
        if let Some(mode) = patch.proof_security_mode {
            tx.execute(
                "UPDATE steps SET proof_security_mode=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    mode.as_i64(),
                    now_ms
                ],
            )?;
        }
        if let Some(mode) = patch.proof_perf_mode {
            tx.execute(
                "UPDATE steps SET proof_perf_mode=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    mode.as_i64(),
                    now_ms
                ],
            )?;
        }
        if let Some(mode) = patch.proof_docs_mode {
            tx.execute(
                "UPDATE steps SET proof_docs_mode=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    mode.as_i64(),
                    now_ms
                ],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.clone()),
            Some(path.clone()),
            "step_defined",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, &task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let (after_detail, after_completed_at_ms) =
            detail::load_step_detail_tx(&tx, workspace.as_str(), &task_id, &step_id, &path)?;
        let after_snapshot =
            detail::step_detail_snapshot_json(&task_id, &after_detail, after_completed_at_ms);

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "step_patch",
                    payload_json: &event_payload_json,
                    before_json: Some(&before_snapshot.to_string()),
                    after_json: Some(&after_snapshot.to_string()),
                    undoable: true,
                    now_ms,
                },
            )?;
        }

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), &task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            &task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
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

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }
}
