#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn step_detail(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        step_id: Option<&str>,
        path: Option<&StepPath>,
    ) -> Result<StepDetail, StoreError> {
        let tx = self.conn.transaction()?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
        let row = tx
            .query_row(
                r#"
                SELECT title, criteria_confirmed, tests_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       completed, blocked, block_reason
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((title, criteria, tests, security, perf, docs, completed, blocked, block_reason)) =
            row
        else {
            return Err(StoreError::StepNotFound);
        };

        let success_criteria =
            step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_criteria")?;
        let tests_list = step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_tests")?;
        let blockers = step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_blockers")?;

        tx.commit()?;
        Ok(StepDetail {
            step_id,
            path,
            title,
            success_criteria,
            tests: tests_list,
            blockers,
            criteria_confirmed: criteria != 0,
            tests_confirmed: tests != 0,
            security_confirmed: security != 0,
            perf_confirmed: perf != 0,
            docs_confirmed: docs != 0,
            completed: completed != 0,
            blocked: blocked != 0,
            block_reason,
        })
    }
}
