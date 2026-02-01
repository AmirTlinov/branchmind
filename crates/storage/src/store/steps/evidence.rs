#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn evidence_capture(
        &mut self,
        workspace: &WorkspaceId,
        request: EvidenceCaptureRequest,
    ) -> Result<EvidenceCaptureResult, StoreError> {
        let EvidenceCaptureRequest {
            task_id,
            expected_revision,
            agent_id,
            selector,
            artifacts,
            checks,
            attachments,
            checkpoints,
        } = request;

        let mut checkpoints = checkpoints;
        checkpoints.retain(|v| !v.trim().is_empty());
        checkpoints.sort();
        checkpoints.dedup();
        for checkpoint in checkpoints.iter() {
            if !matches!(
                checkpoint.as_str(),
                "criteria" | "tests" | "security" | "perf" | "docs"
            ) {
                return Err(StoreError::InvalidInput(
                    "checkpoint must be one of: criteria, tests, security, perf, docs",
                ));
            }
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let (entity_kind, entity_id, path, revision, reasoning_kind) =
            if selector.step_id.is_some() || selector.path.is_some() {
                let task_revision = bump_task_revision_tx(
                    &tx,
                    workspace.as_str(),
                    &task_id,
                    expected_revision,
                    now_ms,
                )?;
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
                (
                    "step".to_string(),
                    step_id,
                    Some(path),
                    task_revision,
                    TaskKind::Task,
                )
            } else if task_id.starts_with("PLAN-") {
                let revision = bump_plan_revision_tx(
                    &tx,
                    workspace.as_str(),
                    &task_id,
                    expected_revision,
                    now_ms,
                )?;
                (
                    "plan".to_string(),
                    task_id.clone(),
                    None,
                    revision,
                    TaskKind::Plan,
                )
            } else if task_id.starts_with("TASK-") {
                let revision = bump_task_revision_tx(
                    &tx,
                    workspace.as_str(),
                    &task_id,
                    expected_revision,
                    now_ms,
                )?;
                (
                    "task".to_string(),
                    task_id.clone(),
                    None,
                    revision,
                    TaskKind::Task,
                )
            } else {
                return Err(StoreError::InvalidInput(
                    "task must start with PLAN- or TASK-",
                ));
            };

        if !artifacts.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_artifacts WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, artifact) in artifacts.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    r#"
                    INSERT INTO evidence_artifacts(
                        workspace, entity_kind, entity_id, ordinal, kind,
                        command, stdout, stderr, exit_code, diff, content, url, external_uri, meta_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                    "#,
                    params![
                        workspace.as_str(),
                        entity_kind,
                        entity_id,
                        ordinal,
                        artifact.kind,
                        artifact.command,
                        artifact.stdout,
                        artifact.stderr,
                        artifact.exit_code,
                        artifact.diff,
                        artifact.content,
                        artifact.url,
                        artifact.external_uri,
                        artifact.meta_json
                    ],
                )?;
            }
        }

        if !checks.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_checks WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, check) in checks.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    "INSERT INTO evidence_checks(workspace, entity_kind, entity_id, ordinal, check_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![workspace.as_str(), entity_kind, entity_id, ordinal, check],
                )?;
            }
        }

        if !attachments.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_attachments WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, attachment) in attachments.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    "INSERT INTO evidence_attachments(workspace, entity_kind, entity_id, ordinal, attachment) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![workspace.as_str(), entity_kind, entity_id, ordinal, attachment],
                )?;
            }
        }

        let artifacts_count = artifacts.len();
        let checks_count = checks.len();
        let attachments_count = attachments.len();
        let event_payload_json = build_evidence_captured_payload(EvidenceCapturedPayloadArgs {
            task_id: &task_id,
            entity_kind: &entity_kind,
            entity_id: &entity_id,
            path: path.as_deref(),
            artifacts_count,
            checks_count,
            attachments_count,
            checkpoints: &checkpoints,
        });
        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: reasoning_kind,
                path: path.clone(),
                event_type: "evidence_captured",
                payload_json: &event_payload_json,
            },
        )?;

        if !checkpoints.is_empty() {
            let event_id = event.event_id();
            for checkpoint in checkpoints.iter() {
                let base_ordinal: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(ordinal), -1) FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND checkpoint=?4",
                    params![workspace.as_str(), entity_kind, entity_id, checkpoint],
                    |row| row.get(0),
                )?;
                let ordinal = base_ordinal + 1;
                tx.execute(
                    r#"
                    INSERT INTO checkpoint_evidence(workspace, entity_kind, entity_id, checkpoint, ordinal, ref)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                    params![
                        workspace.as_str(),
                        entity_kind,
                        entity_id,
                        checkpoint,
                        ordinal,
                        event_id
                    ],
                )?;
            }
        }

        if artifacts_count + checks_count + attachments_count > 0 {
            let event_id = event.event_id();
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.notes_doc,
                DocumentKind::Notes.as_str(),
                now_ms,
            )?;
            let meta_json = build_evidence_mirror_meta_json(EvidenceMirrorMetaTxArgs {
                task_id: &task_id,
                entity_kind: &entity_kind,
                entity_id: &entity_id,
                path: path.as_deref(),
                artifacts_count,
                checks_count,
                attachments_count,
                event_id: event_id.as_str(),
                checkpoints: &checkpoints,
            });
            let mut content = format!("Evidence captured for {entity_kind} {entity_id}");
            if let Some(path) = path.as_deref() {
                content.push_str(&format!(" ({path})"));
            }
            content.push_str(&format!(
                ": artifacts={artifacts_count}, checks={checks_count}, attachments={attachments_count}"
            ));
            if !checkpoints.is_empty() {
                content.push_str(&format!("; checkpoints={}", checkpoints.join(",")));
            }
            tx.execute(
                r#"
                INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, meta_json, content)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.notes_doc,
                    now_ms,
                    DocEntryKind::Note.as_str(),
                    "Evidence captured",
                    meta_json,
                    &content
                ],
            )?;
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.notes_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(EvidenceCaptureResult {
            revision,
            step: path.map(|p| StepRef {
                step_id: entity_id,
                path: p,
            }),
            event,
        })
    }
}
