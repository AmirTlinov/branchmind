#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn step_define(
        &mut self,
        workspace: &WorkspaceId,
        request: StepDefineRequest,
    ) -> Result<StepOpResult, StoreError> {
        let StepDefineRequest {
            task_id,
            expected_revision,
            agent_id,
            selector,
            patch,
        } = request;
        let StepPatch {
            title,
            success_criteria,
            tests,
            blockers,
            next_action,
            stop_criteria,
            proof_tests_mode,
            proof_security_mode,
            proof_perf_mode,
            proof_docs_mode,
        } = patch;

        if title.is_none()
            && success_criteria.is_none()
            && tests.is_none()
            && blockers.is_none()
            && next_action.is_none()
            && stop_criteria.is_none()
            && proof_tests_mode.is_none()
            && proof_security_mode.is_none()
            && proof_perf_mode.is_none()
            && proof_docs_mode.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to define"));
        }

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

        let mut fields = Vec::new();

        if let Some(title) = title {
            tx.execute(
                "UPDATE steps SET title=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, title, now_ms],
            )?;
            fields.push("title");
        }

        if let Some(items) = success_criteria {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET criteria_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("success_criteria");
        }

        if let Some(items) = tests {
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET tests_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("tests");
        }

        if let Some(items) = blockers {
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            fields.push("blockers");
        }

        if let Some(next_action) = next_action {
            tx.execute(
                "UPDATE steps SET next_action=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, next_action, now_ms],
            )?;
            fields.push("next_action");
        }
        if let Some(stop_criteria) = stop_criteria {
            tx.execute(
                "UPDATE steps SET stop_criteria=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, stop_criteria, now_ms],
            )?;
            fields.push("stop_criteria");
        }

        if let Some(mode) = proof_tests_mode {
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
            fields.push("proof_tests_mode");
        }
        if let Some(mode) = proof_security_mode {
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
            fields.push("proof_security_mode");
        }
        if let Some(mode) = proof_perf_mode {
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
            fields.push("proof_perf_mode");
        }
        if let Some(mode) = proof_docs_mode {
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
            fields.push("proof_docs_mode");
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_defined_payload(&task_id, &step_ref, &fields);
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
