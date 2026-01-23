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
                   assignee, domain, phase, component, reasoning_mode, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms, parked_until_ts_ms, stale_after_ms
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
                    reasoning_mode: row.get::<_, String>(13)?,
                    context: row.get(14)?,
                    criteria_confirmed: row.get::<_, i64>(15)? != 0,
                    tests_confirmed: row.get::<_, i64>(16)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(18)? != 0,
                    security_confirmed: row.get::<_, i64>(19)? != 0,
                    perf_confirmed: row.get::<_, i64>(20)? != 0,
                    docs_confirmed: row.get::<_, i64>(21)? != 0,
                    created_at_ms: row.get(22)?,
                    updated_at_ms: row.get(23)?,
                    parked_until_ts_ms: row.get(24)?,
                    stale_after_ms: row.get(25)?,
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
                   assignee, domain, phase, component, reasoning_mode, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms, parked_until_ts_ms, stale_after_ms
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
                    reasoning_mode: row.get::<_, String>(13)?,
                    context: row.get(14)?,
                    criteria_confirmed: row.get::<_, i64>(15)? != 0,
                    tests_confirmed: row.get::<_, i64>(16)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(18)? != 0,
                    security_confirmed: row.get::<_, i64>(19)? != 0,
                    perf_confirmed: row.get::<_, i64>(20)? != 0,
                    docs_confirmed: row.get::<_, i64>(21)? != 0,
                    created_at_ms: row.get(22)?,
                    updated_at_ms: row.get(23)?,
                    parked_until_ts_ms: row.get(24)?,
                    stale_after_ms: row.get(25)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_tasks_for_plan_by_status(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
        status: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description,
                   status, status_manual, priority, blocked,
                   assignee, domain, phase, component, reasoning_mode, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms, parked_until_ts_ms, stale_after_ms
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2 AND status = ?3
            ORDER BY id ASC
            LIMIT ?4 OFFSET ?5
            "#,
        )?;
        let rows = stmt.query_map(
            params![
                workspace.as_str(),
                plan_id,
                status,
                limit as i64,
                offset as i64
            ],
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
                    reasoning_mode: row.get::<_, String>(13)?,
                    context: row.get(14)?,
                    criteria_confirmed: row.get::<_, i64>(15)? != 0,
                    tests_confirmed: row.get::<_, i64>(16)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(18)? != 0,
                    security_confirmed: row.get::<_, i64>(19)? != 0,
                    perf_confirmed: row.get::<_, i64>(20)? != 0,
                    docs_confirmed: row.get::<_, i64>(21)? != 0,
                    created_at_ms: row.get(22)?,
                    updated_at_ms: row.get(23)?,
                    parked_until_ts_ms: row.get(24)?,
                    stale_after_ms: row.get(25)?,
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

    pub fn count_tasks_by_status_for_plan(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
    ) -> Result<std::collections::BTreeMap<String, i64>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT status, COUNT(*)
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2
            GROUP BY status
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str(), plan_id])?;
        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            out.insert(status, count);
        }
        Ok(out)
    }

    pub fn plan_horizon_stats_for_plan(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
        now_ms: i64,
        stale_default_ms: i64,
    ) -> Result<PlanHorizonStats, StoreError> {
        let stale_default_ms = stale_default_ms.max(0);
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              SUM(CASE WHEN status='ACTIVE' THEN 1 ELSE 0 END) AS active,
              SUM(CASE
                    WHEN status='TODO' THEN 1
                    WHEN status='PARKED' AND parked_until_ts_ms IS NOT NULL AND parked_until_ts_ms <= ?3 THEN 1
                    ELSE 0
                  END) AS backlog,
              SUM(CASE
                    WHEN status='PARKED' AND (parked_until_ts_ms IS NULL OR parked_until_ts_ms > ?3) THEN 1
                    ELSE 0
                  END) AS parked,
              SUM(CASE WHEN status='DONE' THEN 1 ELSE 0 END) AS done,
              COUNT(*) AS total,
              SUM(CASE
                    WHEN status IN ('TODO','ACTIVE','PARKED')
                      AND (?3 - updated_at_ms) > COALESCE(stale_after_ms, ?4)
                    THEN 1
                    ELSE 0
                  END) AS stale
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2
            "#,
        )?;
        let stats_row = stmt.query_row(
            params![workspace.as_str(), plan_id, now_ms, stale_default_ms],
            |row| {
                let active: Option<i64> = row.get(0)?;
                let backlog: Option<i64> = row.get(1)?;
                let parked: Option<i64> = row.get(2)?;
                let done: Option<i64> = row.get(3)?;
                let total: i64 = row.get(4)?;
                let stale: Option<i64> = row.get(5)?;
                Ok((
                    active.unwrap_or(0),
                    backlog.unwrap_or(0),
                    parked.unwrap_or(0),
                    done.unwrap_or(0),
                    total,
                    stale.unwrap_or(0),
                ))
            },
        )?;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, parked_until_ts_ms
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2
              AND status = 'PARKED'
              AND parked_until_ts_ms IS NOT NULL
              AND parked_until_ts_ms > ?3
            ORDER BY parked_until_ts_ms ASC, id ASC
            LIMIT 1
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str(), plan_id, now_ms])?;
        let next_wake = if let Some(row) = rows.next()? {
            Some(PlanHorizonWake {
                task_id: row.get(0)?,
                parked_until_ts_ms: row.get(1)?,
            })
        } else {
            None
        };

        let (active, backlog, parked, done, total, stale) = stats_row;
        Ok(PlanHorizonStats {
            active,
            backlog,
            parked,
            done,
            total,
            stale,
            next_wake,
        })
    }
}
