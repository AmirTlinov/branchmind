#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn step_verify(
        &mut self,
        workspace: &WorkspaceId,
        request: StepVerifyRequest,
    ) -> Result<StepOpResult, StoreError> {
        let StepVerifyRequest {
            task_id,
            expected_revision,
            agent_id,
            selector,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        } = request;

        if criteria_confirmed.is_none()
            && tests_confirmed.is_none()
            && security_confirmed.is_none()
            && perf_confirmed.is_none()
            && docs_confirmed.is_none()
        {
            return Err(StoreError::InvalidInput("no checkpoints to verify"));
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
        super::super::lease::enforce_step_lease_tx(
            &tx,
            workspace.as_str(),
            &step_id,
            agent_id.as_deref(),
        )?;
        if let Some(v) = criteria_confirmed {
            tx.execute(
                "UPDATE steps SET criteria_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if v { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        }
        if let Some(v) = tests_confirmed {
            tx.execute(
                "UPDATE steps SET tests_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if v { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        }
        if let Some(v) = security_confirmed {
            tx.execute(
                "UPDATE steps SET security_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if v { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        }
        if let Some(v) = perf_confirmed {
            tx.execute(
                "UPDATE steps SET perf_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if v { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        }
        if let Some(v) = docs_confirmed {
            tx.execute(
                "UPDATE steps SET docs_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if v { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_verified_payload(
            &task_id,
            &step_ref,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        );
        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "step_verified",
                payload_json: &event_payload_json,
            },
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
