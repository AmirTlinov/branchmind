#![forbid(unsafe_code)]

use std::collections::HashSet;

use super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::paths::StepPath;
use rusqlite::params;

impl SqliteStore {
    pub fn list_task_steps(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        parent_path: Option<&StepPath>,
        limit: usize,
    ) -> Result<Vec<StepListRow>, StoreError> {
        let tx = self.conn.transaction()?;
        let subtree_ids = if let Some(path) = parent_path {
            let step_id = resolve_step_id_tx(&tx, workspace.as_str(), task_id, path)?;
            let ids = collect_step_subtree_ids_tx(&tx, workspace.as_str(), task_id, &step_id)?;
            Some(ids.into_iter().collect::<HashSet<_>>())
        } else {
            None
        };

        let raw_rows = {
            let mut stmt = tx.prepare(
                r#"
                SELECT step_id, title, completed, created_at_ms, updated_at_ms, completed_at_ms,
                       criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed,
                       docs_confirmed, blocked, block_reason
                FROM steps
                WHERE workspace=?1 AND task_id=?2
                "#,
            )?;
            let rows = stmt.query_map(params![workspace.as_str(), task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut steps = Vec::new();
        for row in raw_rows {
            let (
                step_id,
                title,
                completed,
                created_at_ms,
                updated_at_ms,
                completed_at_ms,
                criteria,
                tests,
                security,
                perf,
                docs,
                blocked,
                block_reason,
            ) = row;
            if let Some(ref ids) = subtree_ids
                && !ids.contains(&step_id)
            {
                continue;
            }
            let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id, &step_id)?;
            steps.push(StepListRow {
                step_id,
                path,
                title,
                completed: completed != 0,
                created_at_ms,
                updated_at_ms,
                completed_at_ms,
                criteria_confirmed: criteria != 0,
                tests_confirmed: tests != 0,
                security_confirmed: security != 0,
                perf_confirmed: perf != 0,
                docs_confirmed: docs != 0,
                blocked: blocked != 0,
                block_reason,
            });
        }

        steps.sort_by(|a, b| a.path.cmp(&b.path));
        if limit > 0 && steps.len() > limit {
            steps.truncate(limit);
        }

        tx.commit()?;
        Ok(steps)
    }
}
