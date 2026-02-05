#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

const MAX_SEARCH_LIMIT: usize = 200;

impl SqliteStore {
    pub fn list_plans(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<PlanRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, title, contract, contract_json, description, context,
                   status, status_manual, priority, plan_doc, plan_current,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM plans
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
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
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn search_plans(
        &self,
        workspace: &WorkspaceId,
        request: PlansSearchRequest,
    ) -> Result<PlansSearchResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_SEARCH_LIMIT);
        if limit == 0 {
            return Ok(PlansSearchResult {
                plans: Vec::new(),
                has_more: false,
            });
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(PlansSearchResult {
                plans: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;
        let text_like = format!("%{text}%");

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, title, updated_at_ms
            FROM plans
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
            Ok(PlanSearchHit {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at_ms: row.get(2)?,
            })
        })?;
        let mut plans = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = plans.len() > limit;
        plans.truncate(limit);
        Ok(PlansSearchResult { plans, has_more })
    }

    pub fn count_plans(&self, workspace: &WorkspaceId) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM plans WHERE workspace = ?1",
            params![workspace.as_str()],
            |row| row.get(0),
        )?)
    }
}
