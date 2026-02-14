#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

const MAX_THREAD_ID_LEN: usize = 200;
const MAX_AGENT_ID_LEN: usize = 128;
const MAX_KIND_LEN: usize = 48;
const MAX_SUMMARY_LEN: usize = 800;
const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;
const MAX_REFS: usize = 32;
const MAX_REF_ITEM_LEN: usize = 256;
const MAX_PULL_LIMIT: usize = 200;
const MAX_THREADS_STATUS: usize = 80;
const MAX_THREADS_RECENT: usize = 120;
const MAX_LINKS_RECENT: usize = 50;

fn normalize_thread_id(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("thread_id must not be empty"));
    }
    if raw.len() > MAX_THREAD_ID_LEN {
        return Err(StoreError::InvalidInput("thread_id too long"));
    }

    // Normalize separators + trim extra slashes deterministically.
    let normalized = raw.replace('\\', "/");
    let mut parts = Vec::<&str>::new();
    for p in normalized.split('/') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if p == "." {
            continue;
        }
        if p == ".." {
            return Err(StoreError::InvalidInput("thread_id must not contain '..'"));
        }
        parts.push(p);
    }
    if parts.is_empty() {
        return Err(StoreError::InvalidInput("thread_id must not be empty"));
    }

    // Canonicalize dm threads: dm/<a>/<b> where a<=b.
    if parts.len() == 3 && parts[0].eq_ignore_ascii_case("dm") {
        let a = parts[1].trim();
        let b = parts[2].trim();
        if a.is_empty() || b.is_empty() {
            return Err(StoreError::InvalidInput(
                "dm thread_id: agent ids must not be empty",
            ));
        }
        let (x, y) = if a <= b { (a, b) } else { (b, a) };
        return Ok(format!("dm/{x}/{y}"));
    }

    Ok(parts.join("/"))
}

fn normalize_agent_id(raw: &str, field: &'static str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput(field));
    }
    if raw.len() > MAX_AGENT_ID_LEN {
        return Err(StoreError::InvalidInput("agent id too long"));
    }
    Ok(raw.to_string())
}

fn normalize_kind(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("kind must not be empty"));
    }
    if raw.len() > MAX_KIND_LEN {
        return Err(StoreError::InvalidInput("kind too long"));
    }
    Ok(raw.to_string())
}

fn normalize_summary(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("summary must not be empty"));
    }
    if raw.len() > MAX_SUMMARY_LEN {
        return Err(StoreError::InvalidInput("summary too long"));
    }
    Ok(raw.to_string())
}

fn normalize_idempotency_key(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput(
            "idempotency_key must not be empty",
        ));
    }
    if raw.len() > MAX_IDEMPOTENCY_KEY_LEN {
        return Err(StoreError::InvalidInput("idempotency_key too long"));
    }
    Ok(raw.to_string())
}

fn normalize_refs(mut refs: Vec<String>) -> Result<Vec<String>, StoreError> {
    if refs.len() > MAX_REFS {
        refs.truncate(MAX_REFS);
    }
    let mut out = Vec::<String>::new();
    for r in refs {
        let trimmed = r.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_REF_ITEM_LEN {
            out.push(trimmed.chars().take(MAX_REF_ITEM_LEN).collect());
        } else {
            out.push(trimmed.to_string());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn read_bus_message_row(row: &rusqlite::Row<'_>) -> Result<JobBusMessageRow, rusqlite::Error> {
    let refs_json: Option<String> = row.get(8)?;
    let refs = crate::store::anchors::decode_json_string_list(refs_json).unwrap_or_default();
    Ok(JobBusMessageRow {
        seq: row.get(0)?,
        ts_ms: row.get(1)?,
        thread_id: row.get(2)?,
        from_agent_id: row.get(3)?,
        from_job_id: row.get(4)?,
        to_agent_id: row.get(5)?,
        kind: row.get(6)?,
        summary: row.get(7)?,
        refs,
        payload_json: row.get(9)?,
        idempotency_key: row.get(10)?,
    })
}

impl SqliteStore {
    pub fn job_bus_publish(
        &mut self,
        workspace: &WorkspaceId,
        request: JobBusPublishRequest,
    ) -> Result<JobBusPublishResult, StoreError> {
        let now_ms = now_ms();
        let thread_id = normalize_thread_id(&request.thread_id)?;
        let idempotency_key = normalize_idempotency_key(&request.idempotency_key)?;
        let from_agent_id =
            normalize_agent_id(&request.from_agent_id, "from_agent_id must not be empty")?;
        let to_agent_id = request
            .to_agent_id
            .as_deref()
            .map(|s| normalize_agent_id(s, "to_agent_id must not be empty"))
            .transpose()?;
        let kind = normalize_kind(&request.kind)?;
        let summary = normalize_summary(&request.summary)?;
        let refs = normalize_refs(request.refs)?;
        let refs_json = crate::store::anchors::encode_json_string_list(&refs);

        // Ensure payload_json (if present) is valid JSON to keep the store trustworthy.
        let payload_json = match request.payload_json {
            None => None,
            Some(raw) => {
                let raw = raw.trim().to_string();
                if raw.is_empty() {
                    None
                } else {
                    let _ = serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| {
                        StoreError::InvalidInput("payload_json: expected valid JSON")
                    })?;
                    Some(raw)
                }
            }
        };

        let tx = self.conn.transaction()?;

        // Idempotency: ignore duplicates (workspace,idempotency_key) and return the existing row.
        let inserted = tx.execute(
            r#"
            INSERT OR IGNORE INTO job_bus_messages(
              workspace, ts_ms, thread_id, from_agent_id, from_job_id, to_agent_id, kind, summary, refs_json, payload_json, idempotency_key
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                workspace.as_str(),
                now_ms,
                thread_id,
                from_agent_id,
                request.from_job_id.as_deref(),
                to_agent_id.as_deref(),
                kind,
                summary,
                refs_json,
                payload_json.as_deref(),
                idempotency_key
            ],
        )?;

        let row = {
            let mut stmt = tx.prepare(
                r#"
                SELECT
                  seq, ts_ms, thread_id, from_agent_id, from_job_id, to_agent_id, kind, summary, refs_json, payload_json, idempotency_key
                FROM job_bus_messages
                WHERE workspace=?1 AND idempotency_key=?2
                ORDER BY seq DESC
                LIMIT 1
                "#,
            )?;
            stmt.query_row(
                params![workspace.as_str(), &idempotency_key],
                read_bus_message_row,
            )
            .optional()?
            .ok_or(StoreError::InvalidInput(
                "job_bus_publish: failed to read published message",
            ))?
        };

        tx.commit()?;
        Ok(JobBusPublishResult {
            message: row,
            deduped: inserted == 0,
        })
    }

    pub fn job_bus_pull(
        &mut self,
        workspace: &WorkspaceId,
        request: JobBusPullRequest,
    ) -> Result<JobBusPullResult, StoreError> {
        let thread_id = normalize_thread_id(&request.thread_id)?;
        let consumer_id =
            normalize_agent_id(&request.consumer_id, "consumer_id must not be empty")?;
        let limit = request.limit.clamp(1, MAX_PULL_LIMIT);

        let tx = self.conn.transaction()?;
        let mut after_seq = request.after_seq.unwrap_or(0);
        if request.after_seq.is_none() {
            after_seq = tx
                .query_row(
                    "SELECT after_seq FROM job_bus_offsets WHERE workspace=?1 AND consumer_id=?2 AND thread_id=?3",
                    params![workspace.as_str(), consumer_id.as_str(), thread_id.as_str()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
        }

        let mut messages = Vec::<JobBusMessageRow>::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT
                  seq, ts_ms, thread_id, from_agent_id, from_job_id, to_agent_id, kind, summary, refs_json, payload_json, idempotency_key
                FROM job_bus_messages
                WHERE workspace=?1 AND thread_id=?2 AND seq>?3
                ORDER BY seq ASC
                LIMIT ?4
                "#,
            )?;
            let mut rows = stmt.query(params![
                workspace.as_str(),
                thread_id.as_str(),
                after_seq,
                (limit as i64) + 1
            ])?;
            while let Some(row) = rows.next()? {
                messages.push(read_bus_message_row(row)?);
            }
        }
        let has_more = messages.len() > limit;
        if has_more {
            messages.truncate(limit);
        }

        let next_after_seq = messages.last().map(|m| m.seq).unwrap_or(after_seq);
        tx.commit()?;
        Ok(JobBusPullResult {
            messages,
            next_after_seq,
            has_more,
        })
    }

    pub fn job_bus_ack(
        &mut self,
        workspace: &WorkspaceId,
        request: JobBusAckRequest,
    ) -> Result<JobBusAckResult, StoreError> {
        let thread_id = normalize_thread_id(&request.thread_id)?;
        let consumer_id =
            normalize_agent_id(&request.consumer_id, "consumer_id must not be empty")?;
        let after_seq = request.after_seq.max(0);
        let now_ms = now_ms();

        let tx = self.conn.transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT after_seq FROM job_bus_offsets WHERE workspace=?1 AND consumer_id=?2 AND thread_id=?3",
                params![workspace.as_str(), consumer_id.as_str(), thread_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let new_after = existing.unwrap_or(0).max(after_seq);

        tx.execute(
            r#"
            INSERT INTO job_bus_offsets(workspace, consumer_id, thread_id, after_seq, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(workspace, consumer_id, thread_id)
            DO UPDATE SET after_seq=?4, updated_at_ms=?5
            "#,
            params![
                workspace.as_str(),
                consumer_id.as_str(),
                thread_id.as_str(),
                new_after,
                now_ms
            ],
        )?;
        tx.commit()?;

        Ok(JobBusAckResult {
            consumer_id,
            thread_id,
            after_seq: new_after,
            updated_at_ms: now_ms,
        })
    }

    pub fn job_bus_thread_statuses(
        &mut self,
        workspace: &WorkspaceId,
        request: JobBusThreadStatusRequest,
    ) -> Result<JobBusThreadStatusResult, StoreError> {
        let consumer_id =
            normalize_agent_id(&request.consumer_id, "consumer_id must not be empty")?;
        let mut thread_ids = request.thread_ids;
        if thread_ids.len() > MAX_THREADS_STATUS {
            thread_ids.truncate(MAX_THREADS_STATUS);
        }
        let mut normalized = Vec::<String>::new();
        for tid in thread_ids {
            normalized.push(normalize_thread_id(&tid)?);
        }
        normalized.sort();
        normalized.dedup();

        let tx = self.conn.transaction()?;
        let mut rows_out = Vec::<JobBusThreadStatusRow>::new();
        for tid in normalized {
            let after_seq: i64 = tx
                .query_row(
                    "SELECT after_seq FROM job_bus_offsets WHERE workspace=?1 AND consumer_id=?2 AND thread_id=?3",
                    params![workspace.as_str(), consumer_id.as_str(), tid.as_str()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);

            let last: Option<(i64, i64, String, String)> = tx
                .query_row(
                    r#"
                    SELECT seq, ts_ms, kind, summary
                    FROM job_bus_messages
                    WHERE workspace=?1 AND thread_id=?2
                    ORDER BY seq DESC
                    LIMIT 1
                    "#,
                    params![workspace.as_str(), tid.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;

            let unread: i64 = tx.query_row(
                "SELECT COUNT(*) FROM job_bus_messages WHERE workspace=?1 AND thread_id=?2 AND seq>?3",
                params![workspace.as_str(), tid.as_str(), after_seq],
                |row| row.get::<_, i64>(0),
            )?;

            let (last_seq, last_ts_ms, last_kind, last_summary) = match last {
                Some((seq, ts, kind, summary)) => (Some(seq), Some(ts), Some(kind), Some(summary)),
                None => (None, None, None, None),
            };

            rows_out.push(JobBusThreadStatusRow {
                thread_id: tid,
                after_seq,
                unread_count: unread.max(0),
                last_seq,
                last_ts_ms,
                last_kind,
                last_summary,
            });
        }
        tx.commit()?;
        Ok(JobBusThreadStatusResult { rows: rows_out })
    }

    pub fn job_bus_threads_recent(
        &mut self,
        workspace: &WorkspaceId,
        request: JobBusThreadsRecentRequest,
    ) -> Result<JobBusThreadsRecentResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_THREADS_RECENT);
        let tx = self.conn.transaction()?;
        let mut rows_out = Vec::<JobBusThreadsRecentRow>::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT thread_id, MAX(seq) AS last_seq, MAX(ts_ms) AS last_ts_ms
                FROM job_bus_messages
                WHERE workspace=?1
                GROUP BY thread_id
                ORDER BY last_seq DESC
                LIMIT ?2
                "#,
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), limit as i64])?;
            while let Some(row) = rows.next()? {
                rows_out.push(JobBusThreadsRecentRow {
                    thread_id: row.get(0)?,
                    last_seq: row.get(1)?,
                    last_ts_ms: row.get(2)?,
                });
            }
        }
        tx.commit()?;
        Ok(JobBusThreadsRecentResult { rows: rows_out })
    }

    pub fn job_bus_links_recent(
        &mut self,
        workspace: &WorkspaceId,
        limit: usize,
    ) -> Result<Vec<JobBusMessageRow>, StoreError> {
        let limit = limit.clamp(1, MAX_LINKS_RECENT);
        let tx = self.conn.transaction()?;
        let mut out = Vec::<JobBusMessageRow>::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT
                  seq, ts_ms, thread_id, from_agent_id, from_job_id, to_agent_id, kind, summary, refs_json, payload_json, idempotency_key
                FROM job_bus_messages
                WHERE workspace=?1 AND thread_id='workspace/main' AND kind='link'
                ORDER BY seq DESC
                LIMIT ?2
                "#,
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), limit as i64])?;
            while let Some(row) = rows.next()? {
                out.push(read_bus_message_row(row)?);
            }
        }
        tx.commit()?;
        Ok(out)
    }
}
