#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

const MAX_SEARCH_LIMIT: usize = 200;

fn normalize_non_empty(value: &str, field: &'static str) -> Result<String, StoreError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(StoreError::InvalidInput(field));
    }
    Ok(trimmed.to_string())
}

fn parse_plan_slice_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlanSliceRow> {
    Ok(PlanSliceRow {
        plan_id: row.get(0)?,
        slice_id: row.get(1)?,
        slice_task_id: row.get(2)?,
        title: row.get(3)?,
        objective: row.get(4)?,
        status: row.get(5)?,
        budgets_json: row.get(6)?,
        created_at_ms: row.get(7)?,
        updated_at_ms: row.get(8)?,
    })
}

impl SqliteStore {
    pub fn plan_slice_next_id(&mut self, workspace: &WorkspaceId) -> Result<String, StoreError> {
        let tx = self.conn.transaction()?;
        let seq = next_counter_tx(&tx, workspace.as_str(), "slice_seq")?;
        tx.commit()?;
        Ok(format!("SLC-{seq:08X}"))
    }

    pub fn plan_slice_insert(
        &mut self,
        workspace: &WorkspaceId,
        request: PlanSliceInsertRequest,
    ) -> Result<PlanSliceRow, StoreError> {
        let plan_id = normalize_non_empty(&request.plan_id, "plan_id must not be empty")?;
        let slice_id = normalize_non_empty(&request.slice_id, "slice_id must not be empty")?;
        let slice_task_id =
            normalize_non_empty(&request.slice_task_id, "slice_task_id must not be empty")?;
        let title = normalize_non_empty(&request.title, "title must not be empty")?;
        let objective = normalize_non_empty(&request.objective, "objective must not be empty")?;
        let status = normalize_non_empty(&request.status, "status must not be empty")?;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let plan_exists = tx
            .query_row(
                "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), plan_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !plan_exists {
            return Err(StoreError::UnknownId);
        }

        let task_exists = tx
            .query_row(
                "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), slice_task_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !task_exists {
            return Err(StoreError::UnknownId);
        }

        tx.execute(
            r#"
            INSERT INTO plan_slices(
              workspace, plan_id, slice_id, slice_task_id, title, objective, status,
              budgets_json, created_at_ms, updated_at_ms
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                workspace.as_str(),
                plan_id,
                slice_id,
                slice_task_id,
                title,
                objective,
                status,
                request.budgets_json,
                now_ms,
                now_ms
            ],
        )?;

        let row = tx.query_row(
            r#"
            SELECT plan_id, slice_id, slice_task_id, title, objective, status,
                   budgets_json, created_at_ms, updated_at_ms
            FROM plan_slices
            WHERE workspace=?1 AND plan_id=?2 AND slice_id=?3
            "#,
            params![workspace.as_str(), plan_id, slice_id],
            parse_plan_slice_row,
        )?;
        tx.commit()?;
        Ok(row)
    }

    pub fn plan_slice_get(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
        slice_id: &str,
    ) -> Result<Option<PlanSliceRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT plan_id, slice_id, slice_task_id, title, objective, status,
                       budgets_json, created_at_ms, updated_at_ms
                FROM plan_slices
                WHERE workspace=?1 AND plan_id=?2 AND slice_id=?3
                "#,
                params![workspace.as_str(), plan_id.trim(), slice_id.trim()],
                parse_plan_slice_row,
            )
            .optional()?)
    }

    pub fn plan_slice_get_by_slice_id(
        &self,
        workspace: &WorkspaceId,
        slice_id: &str,
    ) -> Result<Option<PlanSliceRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT plan_id, slice_id, slice_task_id, title, objective, status,
                       budgets_json, created_at_ms, updated_at_ms
                FROM plan_slices
                WHERE workspace=?1 AND slice_id=?2
                ORDER BY updated_at_ms DESC
                LIMIT 1
                "#,
                params![workspace.as_str(), slice_id.trim()],
                parse_plan_slice_row,
            )
            .optional()?)
    }

    pub fn search_plan_slices(
        &self,
        workspace: &WorkspaceId,
        request: PlanSlicesSearchRequest,
    ) -> Result<PlanSlicesSearchResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_SEARCH_LIMIT);
        if limit == 0 {
            return Ok(PlanSlicesSearchResult {
                slices: Vec::new(),
                has_more: false,
            });
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(PlanSlicesSearchResult {
                slices: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;
        let text_like = format!("%{text}%");

        let mut stmt = self.conn.prepare(
            r#"
            SELECT plan_id, slice_id, slice_task_id, title, objective, status, updated_at_ms
            FROM plan_slices
            WHERE workspace = ?1
              AND (
                slice_id LIKE ?2 COLLATE NOCASE
                OR title LIKE ?2 COLLATE NOCASE
                OR objective LIKE ?2 COLLATE NOCASE
                OR status LIKE ?2 COLLATE NOCASE
              )
            ORDER BY updated_at_ms DESC, slice_id ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), text_like, query_limit], |row| {
            Ok(PlanSliceSearchHit {
                plan_id: row.get(0)?,
                slice_id: row.get(1)?,
                slice_task_id: row.get(2)?,
                title: row.get(3)?,
                objective: row.get(4)?,
                status: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        })?;
        let mut slices = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = slices.len() > limit;
        slices.truncate(limit);
        Ok(PlanSlicesSearchResult { slices, has_more })
    }

    pub fn plan_slice_update_status(
        &mut self,
        workspace: &WorkspaceId,
        request: PlanSliceStatusUpdateRequest,
    ) -> Result<PlanSliceRow, StoreError> {
        let plan_id = normalize_non_empty(&request.plan_id, "plan_id must not be empty")?;
        let slice_id = normalize_non_empty(&request.slice_id, "slice_id must not be empty")?;
        let status = normalize_non_empty(&request.status, "status must not be empty")?;
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            r#"
            UPDATE plan_slices
            SET status=?4, updated_at_ms=?5
            WHERE workspace=?1 AND plan_id=?2 AND slice_id=?3
            "#,
            params![workspace.as_str(), plan_id, slice_id, status, now_ms],
        )?;
        if changed == 0 {
            return Err(StoreError::UnknownId);
        }
        let row = tx.query_row(
            r#"
            SELECT plan_id, slice_id, slice_task_id, title, objective, status,
                   budgets_json, created_at_ms, updated_at_ms
            FROM plan_slices
            WHERE workspace=?1 AND plan_id=?2 AND slice_id=?3
            "#,
            params![workspace.as_str(), plan_id, slice_id],
            parse_plan_slice_row,
        )?;
        tx.commit()?;
        Ok(row)
    }
}
