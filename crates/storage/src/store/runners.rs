#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

const MAX_RUNNER_ID_LEN: usize = 256;
const MAX_RUNNER_STATUS_LEN: usize = 16;
const MAX_ACTIVE_JOB_ID_LEN: usize = 64;
const MAX_LEASE_TTL_MS: u64 = 300_000; // 5 minutes
const MIN_LEASE_TTL_MS: u64 = 1_000; // 1 second
const MAX_RUNNER_LIST_LIMIT: usize = 200;

fn normalize_runner_id(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("runner_id must not be empty"));
    }
    if raw.len() > MAX_RUNNER_ID_LEN {
        return Err(StoreError::InvalidInput("runner_id is too long"));
    }
    Ok(raw.to_string())
}

fn normalize_runner_status(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("runner.status must not be empty"));
    }
    if raw.len() > MAX_RUNNER_STATUS_LEN {
        return Err(StoreError::InvalidInput("runner.status is too long"));
    }
    let lowered = raw.to_ascii_lowercase();
    if !matches!(lowered.as_str(), "idle" | "live") {
        return Err(StoreError::InvalidInput(
            "runner.status is invalid (expected idle|live)",
        ));
    }
    Ok(lowered)
}

fn normalize_runner_status_filter(raw: Option<String>) -> Result<Option<String>, StoreError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(normalize_runner_status(trimmed)?))
}

fn normalize_active_job_id(raw: Option<String>) -> Result<Option<String>, StoreError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.len() > MAX_ACTIVE_JOB_ID_LEN {
        return Err(StoreError::InvalidInput("runner.active_job_id is too long"));
    }
    if !raw.starts_with("JOB-") {
        return Err(StoreError::InvalidInput(
            "runner.active_job_id must start with JOB-",
        ));
    }
    Ok(Some(raw.to_string()))
}

fn read_runner_lease_row(row: &rusqlite::Row<'_>) -> Result<RunnerLeaseRow, rusqlite::Error> {
    Ok(RunnerLeaseRow {
        runner_id: row.get(0)?,
        status: row.get(1)?,
        active_job_id: row.get(2)?,
        lease_expires_at_ms: row.get(3)?,
        created_at_ms: row.get(4)?,
        updated_at_ms: row.get(5)?,
    })
}

fn read_runner_lease_get_row(
    row: &rusqlite::Row<'_>,
) -> Result<RunnerLeaseGetResult, rusqlite::Error> {
    Ok(RunnerLeaseGetResult {
        lease: RunnerLeaseRow {
            runner_id: row.get(0)?,
            status: row.get(1)?,
            active_job_id: row.get(2)?,
            lease_expires_at_ms: row.get(3)?,
            created_at_ms: row.get(4)?,
            updated_at_ms: row.get(5)?,
        },
        meta_json: row.get(6)?,
    })
}

impl SqliteStore {
    pub fn runner_lease_get(
        &self,
        workspace: &WorkspaceId,
        request: RunnerLeaseGetRequest,
    ) -> Result<Option<RunnerLeaseGetResult>, StoreError> {
        let runner_id = normalize_runner_id(&request.runner_id)?;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT runner_id, status, active_job_id, lease_expires_at_ms, created_at_ms, updated_at_ms, meta_json
            FROM runner_leases
            WHERE workspace=?1 AND runner_id=?2
            "#,
        )?;
        Ok(stmt
            .query_row(
                params![workspace.as_str(), runner_id.as_str()],
                read_runner_lease_get_row,
            )
            .optional()?)
    }

    pub fn runner_lease_upsert(
        &mut self,
        workspace: &WorkspaceId,
        request: RunnerLeaseUpsertRequest,
    ) -> Result<RunnerLeaseRow, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let runner_id = normalize_runner_id(&request.runner_id)?;
        let status = normalize_runner_status(&request.status)?;
        let active_job_id = normalize_active_job_id(request.active_job_id)?;

        let ttl_ms = request
            .lease_ttl_ms
            .clamp(MIN_LEASE_TTL_MS, MAX_LEASE_TTL_MS);
        let lease_expires_at_ms = now_ms.saturating_add(ttl_ms.min(i64::MAX as u64) as i64);

        // Preserve created_at_ms on updates to keep the "session" start visible.
        let existing_created: Option<i64> = tx
            .query_row(
                "SELECT created_at_ms FROM runner_leases WHERE workspace=?1 AND runner_id=?2",
                params![workspace.as_str(), runner_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let created_at_ms = existing_created.unwrap_or(now_ms);

        tx.execute(
            r#"
            INSERT INTO runner_leases(
              workspace, runner_id, status, active_job_id, lease_expires_at_ms,
              created_at_ms, updated_at_ms, meta_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(workspace, runner_id) DO UPDATE SET
              status=excluded.status,
              active_job_id=excluded.active_job_id,
              lease_expires_at_ms=excluded.lease_expires_at_ms,
              updated_at_ms=excluded.updated_at_ms,
              meta_json=excluded.meta_json
            "#,
            params![
                workspace.as_str(),
                runner_id.as_str(),
                status.as_str(),
                active_job_id.as_deref(),
                lease_expires_at_ms,
                created_at_ms,
                now_ms,
                request.meta_json,
            ],
        )?;

        tx.commit()?;
        Ok(RunnerLeaseRow {
            runner_id,
            status,
            active_job_id,
            lease_expires_at_ms,
            created_at_ms,
            updated_at_ms: now_ms,
        })
    }

    pub fn runner_status_snapshot(
        &self,
        workspace: &WorkspaceId,
        now_ms: i64,
    ) -> Result<RunnerStatusSnapshot, StoreError> {
        let mut live = Vec::<RunnerLeaseRow>::new();
        let mut idle = Vec::<RunnerLeaseRow>::new();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT runner_id, status, active_job_id, lease_expires_at_ms, created_at_ms, updated_at_ms
            FROM runner_leases
            WHERE workspace=?1 AND lease_expires_at_ms > ?2
            ORDER BY lease_expires_at_ms DESC, runner_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), now_ms], read_runner_lease_row)?;
        for row in rows {
            let row = row?;
            match row.status.as_str() {
                "live" => live.push(row),
                "idle" => idle.push(row),
                _ => {}
            }
        }

        let live_count = live.len();
        let idle_count = idle.len();
        let offline_count: usize = self
            .conn
            .query_row(
                r#"
            SELECT COUNT(*)
            FROM runner_leases
            WHERE workspace=?1 AND lease_expires_at_ms <= ?2
            "#,
                params![workspace.as_str(), now_ms],
                |row| row.get::<_, i64>(0),
            )?
            .max(0) as usize;

        let (status, chosen) = if live_count > 0 {
            ("live", live.first().cloned())
        } else if idle_count > 0 {
            ("idle", idle.first().cloned())
        } else {
            ("offline", None)
        };

        Ok(RunnerStatusSnapshot {
            status: status.to_string(),
            live_count,
            idle_count,
            offline_count,
            runner_id: chosen.as_ref().map(|r| r.runner_id.clone()),
            active_job_id: chosen.as_ref().and_then(|r| r.active_job_id.clone()),
            lease_expires_at_ms: chosen.as_ref().map(|r| r.lease_expires_at_ms),
        })
    }

    pub fn runner_leases_list_active(
        &self,
        workspace: &WorkspaceId,
        now_ms: i64,
        request: RunnerLeasesListRequest,
    ) -> Result<RunnerLeasesListResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_RUNNER_LIST_LIMIT);
        let limit_plus = limit.saturating_add(1) as i64;
        let status = normalize_runner_status_filter(request.status)?;

        let mut runners = Vec::<RunnerLeaseRow>::new();
        let mut stmt = self.conn.prepare(
            r#"
            SELECT runner_id, status, active_job_id, lease_expires_at_ms, created_at_ms, updated_at_ms
            FROM runner_leases
            WHERE workspace=?1
              AND lease_expires_at_ms > ?2
              AND (?3 IS NULL OR status=?3)
            ORDER BY
              CASE status WHEN 'live' THEN 0 WHEN 'idle' THEN 1 ELSE 2 END,
              lease_expires_at_ms DESC,
              runner_id ASC
            LIMIT ?4
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), now_ms, status.as_deref(), limit_plus],
            read_runner_lease_row,
        )?;
        for row in rows {
            runners.push(row?);
        }

        let has_more = runners.len() > limit;
        runners.truncate(limit);

        Ok(RunnerLeasesListResult { runners, has_more })
    }

    pub fn runner_leases_list_offline_recent(
        &self,
        workspace: &WorkspaceId,
        now_ms: i64,
        request: RunnerLeasesListOfflineRequest,
    ) -> Result<RunnerLeasesListOfflineResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_RUNNER_LIST_LIMIT);
        let limit_plus = limit.saturating_add(1) as i64;

        let mut runners = Vec::<RunnerLeaseRow>::new();
        let mut stmt = self.conn.prepare(
            r#"
            SELECT runner_id, status, active_job_id, lease_expires_at_ms, created_at_ms, updated_at_ms
            FROM runner_leases
            WHERE workspace=?1
              AND lease_expires_at_ms <= ?2
            ORDER BY
              updated_at_ms DESC,
              runner_id ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), now_ms, limit_plus],
            read_runner_lease_row,
        )?;
        for row in rows {
            runners.push(row?);
        }

        let has_more = runners.len() > limit;
        runners.truncate(limit);

        Ok(RunnerLeasesListOfflineResult { runners, has_more })
    }
}
