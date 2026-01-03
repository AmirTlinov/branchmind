#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn task_steps_summary(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
    ) -> Result<TaskStepSummary, StoreError> {
        let tx = self.conn.transaction()?;

        let exists = tx
            .query_row(
                "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), task_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(StoreError::UnknownId);
        }

        let total_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let completed_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=1",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let open_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_criteria: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND criteria_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_tests: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND tests_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_security: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.security_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='security'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='security'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_perf: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.perf_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='perf'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='perf'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_docs: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.docs_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='docs'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='docs'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let missing_proof_tests: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.proof_tests_mode=2
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_notes n
                WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='tests'
              )
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_evidence e
                WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='tests'
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let missing_proof_security: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.proof_security_mode=2
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_notes n
                WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='security'
              )
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_evidence e
                WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='security'
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let missing_proof_perf: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.proof_perf_mode=2
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_notes n
                WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='perf'
              )
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_evidence e
                WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='perf'
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let missing_proof_docs: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.proof_docs_mode=2
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_notes n
                WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='docs'
              )
              AND NOT EXISTS (
                SELECT 1 FROM checkpoint_evidence e
                WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='docs'
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let first_open_row = tx
            .query_row(
                r#"
                SELECT step_id, title, completed, criteria_confirmed, tests_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       proof_tests_mode, proof_security_mode, proof_perf_mode, proof_docs_mode
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND completed=0
                ORDER BY created_at_ms ASC
                LIMIT 1
                "#,
                params![workspace.as_str(), task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                    ))
                },
            )
            .optional()?;

        let first_open = if let Some((
            step_id,
            title,
            completed,
            criteria,
            tests,
            security,
            perf,
            docs,
            proof_tests_mode_raw,
            proof_security_mode_raw,
            proof_perf_mode_raw,
            proof_docs_mode_raw,
        )) = first_open_row
        {
            let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id, &step_id)
                .unwrap_or_else(|_| "s:?".to_string());

            let require_security =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?;
            let require_perf =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?;
            let require_docs =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?;

            let proof_tests_mode = ProofMode::from_i64(proof_tests_mode_raw);
            let proof_security_mode = ProofMode::from_i64(proof_security_mode_raw);
            let proof_perf_mode = ProofMode::from_i64(proof_perf_mode_raw);
            let proof_docs_mode = ProofMode::from_i64(proof_docs_mode_raw);

            let proof_tests_present =
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "tests")?;
            let proof_security_present =
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "security")?;
            let proof_perf_present =
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?;
            let proof_docs_present =
                checkpoint_proof_exists_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?;

            Some(StepStatus {
                step_id,
                path,
                title,
                completed: completed != 0,
                criteria_confirmed: criteria != 0,
                tests_confirmed: tests != 0,
                security_confirmed: security != 0,
                perf_confirmed: perf != 0,
                docs_confirmed: docs != 0,
                require_security,
                require_perf,
                require_docs,
                proof_tests_mode,
                proof_security_mode,
                proof_perf_mode,
                proof_docs_mode,
                proof_tests_present,
                proof_security_present,
                proof_perf_present,
                proof_docs_present,
            })
        } else {
            None
        };

        tx.commit()?;
        Ok(TaskStepSummary {
            total_steps,
            completed_steps,
            open_steps,
            missing_criteria,
            missing_tests,
            missing_security,
            missing_perf,
            missing_docs,
            missing_proof_tests,
            missing_proof_security,
            missing_proof_perf,
            missing_proof_docs,
            first_open,
        })
    }

    pub fn task_last_completed_step_id(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let tx = self.conn.transaction()?;
        let step_id = tx
            .query_row(
                r#"
                SELECT step_id
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND completed=1
                ORDER BY completed_at_ms DESC, updated_at_ms DESC, created_at_ms DESC
                LIMIT 1
                "#,
                params![workspace.as_str(), task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        tx.commit()?;
        Ok(step_id)
    }

    pub fn task_open_blockers(
        &self,
        workspace: &WorkspaceId,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT b.text
            FROM step_blockers b
            JOIN steps s
              ON s.workspace = b.workspace AND s.step_id = b.step_id
            WHERE s.workspace = ?1 AND s.task_id = ?2 AND s.completed = 0
            ORDER BY s.created_at_ms ASC, b.ordinal ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), task_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn task_items_list(
        &mut self,
        workspace: &WorkspaceId,
        entity_kind: &str,
        entity_id: &str,
        field: &str,
    ) -> Result<Vec<String>, StoreError> {
        let tx = self.conn.transaction()?;
        let items = task_items_list_tx(&tx, workspace.as_str(), entity_kind, entity_id, field)?;
        tx.commit()?;
        Ok(items)
    }
}
