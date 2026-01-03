#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};
use serde_json::json;

impl SqliteStore {
    pub fn step_progress(
        &mut self,
        workspace: &WorkspaceId,
        request: StepProgressRequest,
    ) -> Result<StepOpResult, StoreError> {
        let StepProgressRequest {
            task_id,
            expected_revision,
            selector,
            completed,
            force,
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

        let row = tx
            .query_row(
                "SELECT completed, completed_at_ms, criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed, proof_tests_mode, proof_security_mode, proof_perf_mode, proof_docs_mode FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            already_completed,
            completed_at_ms,
            criteria,
            tests,
            security,
            perf,
            docs,
            proof_tests_mode_raw,
            proof_security_mode_raw,
            proof_perf_mode_raw,
            proof_docs_mode_raw,
        )) = row
        else {
            return Err(StoreError::StepNotFound);
        };

        let before_snapshot = json!({
            "task": task_id.as_str(),
            "step_id": step_id,
            "path": path.to_string(),
            "completed": already_completed != 0,
            "completed_at_ms": completed_at_ms
        });

        if completed {
            if already_completed != 0 {
                return Err(StoreError::InvalidInput("step already completed"));
            }
            let proof_tests_mode = ProofMode::from_i64(proof_tests_mode_raw);
            let proof_security_mode = ProofMode::from_i64(proof_security_mode_raw);
            let proof_perf_mode = ProofMode::from_i64(proof_perf_mode_raw);
            let proof_docs_mode = ProofMode::from_i64(proof_docs_mode_raw);
            let require_security =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?
                    || proof_security_mode == ProofMode::Require;
            let require_perf =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?
                    || proof_perf_mode == ProofMode::Require;
            let require_docs =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?
                    || proof_docs_mode == ProofMode::Require;

            if !force
                && (criteria == 0
                    || tests == 0
                    || (require_security && security == 0)
                    || (require_perf && perf == 0)
                    || (require_docs && docs == 0))
            {
                return Err(StoreError::CheckpointsNotConfirmed {
                    criteria: criteria == 0,
                    tests: tests == 0,
                    security: require_security && security == 0,
                    perf: require_perf && perf == 0,
                    docs: require_docs && docs == 0,
                });
            }
            if !force {
                let require_proof_tests = proof_tests_mode == ProofMode::Require;
                let require_proof_security = proof_security_mode == ProofMode::Require;
                let require_proof_perf = proof_perf_mode == ProofMode::Require;
                let require_proof_docs = proof_docs_mode == ProofMode::Require;
                if require_proof_tests
                    || require_proof_security
                    || require_proof_perf
                    || require_proof_docs
                {
                    let has_tests_proof = if require_proof_tests {
                        checkpoint_proof_exists_tx(
                            &tx,
                            workspace.as_str(),
                            "step",
                            &step_id,
                            "tests",
                        )?
                    } else {
                        true
                    };
                    let has_security_proof = if require_proof_security {
                        checkpoint_proof_exists_tx(
                            &tx,
                            workspace.as_str(),
                            "step",
                            &step_id,
                            "security",
                        )?
                    } else {
                        true
                    };
                    let has_perf_proof = if require_proof_perf {
                        checkpoint_proof_exists_tx(
                            &tx,
                            workspace.as_str(),
                            "step",
                            &step_id,
                            "perf",
                        )?
                    } else {
                        true
                    };
                    let has_docs_proof = if require_proof_docs {
                        checkpoint_proof_exists_tx(
                            &tx,
                            workspace.as_str(),
                            "step",
                            &step_id,
                            "docs",
                        )?
                    } else {
                        true
                    };
                    if !has_tests_proof || !has_security_proof || !has_perf_proof || !has_docs_proof
                    {
                        return Err(StoreError::ProofMissing {
                            tests: !has_tests_proof,
                            security: !has_security_proof,
                            perf: !has_perf_proof,
                            docs: !has_docs_proof,
                        });
                    }
                }
            }
            tx.execute(
                "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        } else {
            tx.execute(
                "UPDATE steps SET completed=0, completed_at_ms=NULL, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }

        let after_snapshot = json!({
            "task": task_id.as_str(),
            "step_id": step_id,
            "path": path.to_string(),
            "completed": completed,
            "completed_at_ms": if completed { Some(now_ms) } else { None }
        });

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let (event_type, event_payload_json) = if completed {
            ("step_done", build_step_done_payload(&task_id, &step_ref))
        } else {
            (
                "step_reopened",
                build_step_reopened_payload(&task_id, &step_ref, force),
            )
        };
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.clone()),
            Some(path.clone()),
            event_type,
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

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "step_progress",
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

    pub fn step_block_set(
        &mut self,
        workspace: &WorkspaceId,
        request: StepBlockSetRequest,
    ) -> Result<StepOpResult, StoreError> {
        let StepBlockSetRequest {
            task_id,
            expected_revision,
            selector,
            blocked,
            reason,
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

        let row = tx
            .query_row(
                "SELECT blocked, block_reason FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let Some((before_blocked, before_reason)) = row else {
            return Err(StoreError::StepNotFound);
        };

        let before_snapshot = json!({
            "task": task_id.as_str(),
            "step_id": step_id,
            "path": path.to_string(),
            "blocked": before_blocked != 0,
            "block_reason": before_reason
        });

        let payload_reason = reason.clone();
        tx.execute(
            "UPDATE steps SET blocked=?4, block_reason=?5, updated_at_ms=?6 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![
                workspace.as_str(),
                task_id,
                step_id,
                if blocked { 1i64 } else { 0i64 },
                if blocked { reason.clone() } else { None::<String> },
                now_ms
            ],
        )?;

        let after_snapshot = json!({
            "task": task_id.as_str(),
            "step_id": step_id,
            "path": path.to_string(),
            "blocked": blocked,
            "block_reason": if blocked { reason.clone() } else { None }
        });

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json =
            build_step_block_payload(&task_id, &step_ref, blocked, payload_reason.as_deref());
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.clone()),
            Some(path.clone()),
            if blocked {
                "step_blocked"
            } else {
                "step_unblocked"
            },
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

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "step_block_set",
                    payload_json: &event_payload_json,
                    before_json: Some(&before_snapshot.to_string()),
                    after_json: Some(&after_snapshot.to_string()),
                    undoable: true,
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
}
