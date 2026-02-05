#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{params, params_from_iter};

const MAX_SEARCH_LIMIT: usize = 200;
const MAX_CURSOR_LIST_LIMIT: usize = 2000;

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

    pub fn list_tasks_for_plan_cursor(
        &self,
        workspace: &WorkspaceId,
        request: TasksListForPlanCursorRequest,
    ) -> Result<TasksListForPlanCursorResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_CURSOR_LIST_LIMIT);
        let cursor = request.cursor.as_deref();

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
              AND parent_plan_id = ?2
              AND (?3 IS NULL OR id > ?3)
            ORDER BY id ASC
            LIMIT ?4
            "#,
        )?;

        let fetch_limit = (limit + 1) as i64;
        let rows = stmt.query_map(
            params![workspace.as_str(), request.plan_id, cursor, fetch_limit],
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

        let mut tasks = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = tasks.len() > limit;
        if has_more {
            tasks.truncate(limit);
        }
        let next_cursor = if has_more {
            tasks.last().map(|task| task.id.clone())
        } else {
            None
        };

        Ok(TasksListForPlanCursorResult {
            tasks,
            has_more,
            next_cursor,
        })
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

    pub fn search_tasks(
        &self,
        workspace: &WorkspaceId,
        request: TasksSearchRequest,
    ) -> Result<TasksSearchResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_SEARCH_LIMIT);
        if limit == 0 {
            return Ok(TasksSearchResult {
                tasks: Vec::new(),
                has_more: false,
            });
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(TasksSearchResult {
                tasks: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;
        let text_like = format!("%{text}%");

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, parent_plan_id, title, updated_at_ms
            FROM tasks
            WHERE workspace = ?1
              AND (
                id LIKE ?2 COLLATE NOCASE
                OR title LIKE ?2 COLLATE NOCASE
                OR description LIKE ?2 COLLATE NOCASE
                OR context LIKE ?2 COLLATE NOCASE
              )
            ORDER BY updated_at_ms DESC, id ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), text_like, query_limit], |row| {
            Ok(TaskSearchHit {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                title: row.get(2)?,
                updated_at_ms: row.get(3)?,
            })
        })?;
        let mut tasks = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = tasks.len() > limit;
        tasks.truncate(limit);
        Ok(TasksSearchResult { tasks, has_more })
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

    pub fn count_tasks_by_status_for_plans(
        &self,
        workspace: &WorkspaceId,
        plan_ids: &[String],
    ) -> Result<
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, i64>>,
        StoreError,
    > {
        if plan_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }

        let mut placeholders = String::new();
        for idx in 0..plan_ids.len() {
            if idx > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }

        let sql = format!(
            r#"
            SELECT parent_plan_id, status, COUNT(*)
            FROM tasks
            WHERE workspace = ? AND parent_plan_id IN ({placeholders})
            GROUP BY parent_plan_id, status
            "#,
            placeholders = placeholders,
        );

        let mut params = Vec::<rusqlite::types::Value>::new();
        params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
        for plan_id in plan_ids {
            params.push(rusqlite::types::Value::Text(plan_id.clone()));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(params))?;

        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next()? {
            let plan_id: String = row.get(0)?;
            let status: String = row.get(1)?;
            let count: i64 = row.get(2)?;
            out.entry(plan_id)
                .or_insert_with(std::collections::BTreeMap::new)
                .insert(status, count);
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
