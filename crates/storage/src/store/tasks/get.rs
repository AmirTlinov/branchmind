#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn get_plan(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<PlanRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, title, contract, contract_json, description, context,
                       status, status_manual, priority, plan_doc, plan_current,
                       criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       created_at_ms, updated_at_ms
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(PlanRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        title: row.get(2)?,
                        contract: row.get(3)?,
                        contract_json: row.get(4)?,
                        description: row.get(5)?,
                        context: row.get(6)?,
                        status: row.get(7)?,
                        status_manual: row.get::<_, i64>(8)? != 0,
                        priority: row.get(9)?,
                        plan_doc: row.get(10)?,
                        plan_current: row.get(11)?,
                        criteria_confirmed: row.get::<_, i64>(12)? != 0,
                        tests_confirmed: row.get::<_, i64>(13)? != 0,
                        criteria_auto_confirmed: row.get::<_, i64>(14)? != 0,
                        tests_auto_confirmed: row.get::<_, i64>(15)? != 0,
                        security_confirmed: row.get::<_, i64>(16)? != 0,
                        perf_confirmed: row.get::<_, i64>(17)? != 0,
                        docs_confirmed: row.get::<_, i64>(18)? != 0,
                        created_at_ms: row.get(19)?,
                        updated_at_ms: row.get(20)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn find_plan_id_by_title(
        &self,
        workspace: &WorkspaceId,
        title: &str,
    ) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id
                FROM plans
                WHERE workspace = ?1 AND title = ?2
                ORDER BY created_at_ms ASC, id ASC
                LIMIT 1
                "#,
                params![workspace.as_str(), title],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn get_task(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<TaskRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, parent_plan_id, title, description,
                       status, status_manual, priority, blocked,
                       assignee, domain, phase, component, reasoning_mode, context,
                       criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       created_at_ms, updated_at_ms, parked_until_ts_ms, stale_after_ms
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
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
            )
            .optional()?)
    }
}
