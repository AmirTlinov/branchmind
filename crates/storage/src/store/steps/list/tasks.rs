#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn list_tasks(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description,
                   status, status_manual, priority, blocked,
                   assignee, domain, phase, component, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM tasks
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
            |row| {
                Ok(TaskRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_plan_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    status: row.get(5)?,
                    status_manual: row.get::<_, i64>(6)? != 0,
                    priority: row.get(7)?,
                    blocked: row.get::<_, i64>(8)? != 0,
                    assignee: row.get(9)?,
                    domain: row.get(10)?,
                    phase: row.get(11)?,
                    component: row.get(12)?,
                    context: row.get(13)?,
                    criteria_confirmed: row.get::<_, i64>(14)? != 0,
                    tests_confirmed: row.get::<_, i64>(15)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(16)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    security_confirmed: row.get::<_, i64>(18)? != 0,
                    perf_confirmed: row.get::<_, i64>(19)? != 0,
                    docs_confirmed: row.get::<_, i64>(20)? != 0,
                    created_at_ms: row.get(21)?,
                    updated_at_ms: row.get(22)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_tasks_for_plan(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description,
                   status, status_manual, priority, blocked,
                   assignee, domain, phase, component, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2
            ORDER BY id ASC
            LIMIT ?3 OFFSET ?4
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), plan_id, limit as i64, offset as i64],
            |row| {
                Ok(TaskRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_plan_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    status: row.get(5)?,
                    status_manual: row.get::<_, i64>(6)? != 0,
                    priority: row.get(7)?,
                    blocked: row.get::<_, i64>(8)? != 0,
                    assignee: row.get(9)?,
                    domain: row.get(10)?,
                    phase: row.get(11)?,
                    component: row.get(12)?,
                    context: row.get(13)?,
                    criteria_confirmed: row.get::<_, i64>(14)? != 0,
                    tests_confirmed: row.get::<_, i64>(15)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(16)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    security_confirmed: row.get::<_, i64>(18)? != 0,
                    perf_confirmed: row.get::<_, i64>(19)? != 0,
                    docs_confirmed: row.get::<_, i64>(20)? != 0,
                    created_at_ms: row.get(21)?,
                    updated_at_ms: row.get(22)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn count_tasks(&self, workspace: &WorkspaceId) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE workspace = ?1",
            params![workspace.as_str()],
            |row| row.get(0),
        )?)
    }
}
