#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn step_close(
        &mut self,
        workspace: &WorkspaceId,
        request: StepCloseRequest,
    ) -> Result<StepCloseResult, StoreError> {
        let StepCloseRequest {
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

        let row = tx
            .query_row(
                "SELECT completed, criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed, proof_tests_mode, proof_security_mode, proof_perf_mode, proof_docs_mode FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )
            .optional()?;

        let Some((completed, _, _, _, _, _, _, _, _, _)) = row else {
            return Err(StoreError::StepNotFound);
        };
        if completed != 0 {
            return Err(StoreError::InvalidInput("step already completed"));
        }

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

        let (
            criteria_now,
            tests_now,
            security_now,
            perf_now,
            docs_now,
            proof_tests_mode_raw,
            proof_security_mode_raw,
            proof_perf_mode_raw,
            proof_docs_mode_raw,
        ) = tx
            .query_row(
                "SELECT criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed, proof_tests_mode, proof_security_mode, proof_perf_mode, proof_docs_mode FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::StepNotFound)?;

        let proof_tests_mode = ProofMode::from_i64(proof_tests_mode_raw);
        let proof_security_mode = ProofMode::from_i64(proof_security_mode_raw);
        let proof_perf_mode = ProofMode::from_i64(proof_perf_mode_raw);
        let proof_docs_mode = ProofMode::from_i64(proof_docs_mode_raw);

        let require_security = security_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?
            || proof_security_mode == ProofMode::Require;
        let require_perf = perf_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?
            || proof_perf_mode == ProofMode::Require;
        let require_docs = docs_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?
            || proof_docs_mode == ProofMode::Require;

        if criteria_now == 0
            || tests_now == 0
            || (require_security && security_now == 0)
            || (require_perf && perf_now == 0)
            || (require_docs && docs_now == 0)
        {
            return Err(StoreError::CheckpointsNotConfirmed {
                criteria: criteria_now == 0,
                tests: tests_now == 0,
                security: require_security && security_now == 0,
                perf: require_perf && perf_now == 0,
                docs: require_docs && docs_now == 0,
            });
        }

        let require_proof_tests = proof_tests_mode == ProofMode::Require;
        let require_proof_security = proof_security_mode == ProofMode::Require;
        let require_proof_perf = proof_perf_mode == ProofMode::Require;
        let require_proof_docs = proof_docs_mode == ProofMode::Require;

        if require_proof_tests || require_proof_security || require_proof_perf || require_proof_docs
        {
            let has_tests_proof = if require_proof_tests {
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "tests")?
            } else {
                true
            };
            let has_security_proof = if require_proof_security {
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "security")?
            } else {
                true
            };
            let has_perf_proof = if require_proof_perf {
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?
            } else {
                true
            };
            let has_docs_proof = if require_proof_docs {
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?
            } else {
                true
            };

            if !has_tests_proof || !has_security_proof || !has_perf_proof || !has_docs_proof {
                return Err(StoreError::ProofMissing {
                    tests: !has_tests_proof,
                    security: !has_security_proof,
                    perf: !has_perf_proof,
                    docs: !has_docs_proof,
                });
            }
        }

        tx.execute(
            "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let verify_payload_json = build_step_verified_payload(
            &task_id,
            &step_ref,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        );
        let verify_event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.clone()),
            Some(path.clone()),
            "step_verified",
            &verify_payload_json,
        )?;
        let done_payload_json = build_step_done_payload(&task_id, &step_ref);
        let done_event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.clone()),
            Some(path.clone()),
            "step_done",
            &done_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, &task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &verify_event,
        )?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &done_event,
        )?;

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), &task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &done_event,
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
        Ok(StepCloseResult {
            task_revision,
            step: step_ref,
            events: vec![verify_event, done_event],
        })
    }
}
