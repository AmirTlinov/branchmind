#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params, params_from_iter};
use serde_json::{Map as JsonMap, Value as JsonValue};

mod artifacts;

const MAX_JOB_TITLE_LEN: usize = 200;
const MAX_JOB_PROMPT_LEN: usize = 50_000;
const MAX_JOB_KIND_LEN: usize = 64;
const MAX_JOB_RUNNER_LEN: usize = 128;
// Pipeline summaries can carry strict contract artifacts (scout/builder/validator JSON).
// Keep this comfortably above typical flagship packs to avoid JSON truncation drift.
const MAX_JOB_SUMMARY_LEN: usize = 128_000;
const MAX_JOB_ARTIFACT_LEN: usize = 512_000;
const MAX_ARTIFACTS_PER_JOB: usize = 8;
const MAX_ARTIFACT_KEY_LEN: usize = 128;
const MAX_JOB_CLAIM_TTL_MS: u64 = 300_000; // 5 minutes
const MIN_JOB_CLAIM_TTL_MS: u64 = 1_000; // 1 second
const MAX_EVENT_KIND_LEN: usize = 32;
const MAX_EVENT_MESSAGE_LEN: usize = 400;
const MAX_EVENT_REFS: usize = 32;
const MAX_EVENT_REFS_ITEM_LEN: usize = 128;
const MAX_LIST_LIMIT: usize = 200;
const MAX_OPEN_EVENTS: usize = 200;
const MAX_RADAR_SCAN_EVENTS: usize = 20;
const MAX_TAIL_EVENTS: usize = 200;

fn is_runner_internal_message(message: &str) -> bool {
    message
        .trim_start()
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("runner:"))
}

fn pick_last_meaningful_job_event(events_newest_first: &[JobEventRow]) -> Option<JobEventRow> {
    // Prefer the newest *actionable* event for daily supervision (manager inbox):
    // - if a question is outstanding (question > manager), show the question,
    // - if a proof gate is outstanding (proof_gate > max(checkpoint, manager_with_proof)), show it,
    // - if an error is outstanding (error > checkpoint), show it,
    // - otherwise show the newest non-heartbeat, non-runner-internal event (including progress),
    //   so the UX doesn't get "stuck" on a stale historical error.
    //
    // NOTE: `events_newest_first` is newest-first by seq.
    let last_checkpoint_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "checkpoint")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_manager_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "manager")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_manager_proof_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "manager" && !e.refs.is_empty())
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_question_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "question")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_error_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "error")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_proof_gate_seq = events_newest_first
        .iter()
        .find(|e| e.kind == "proof_gate")
        .map(|e| e.seq)
        .unwrap_or(0);

    let needs_manager = last_question_seq > last_manager_seq;
    let needs_proof = last_proof_gate_seq > last_checkpoint_seq.max(last_manager_proof_seq);
    let has_error = last_error_seq > last_checkpoint_seq;

    if needs_manager {
        return events_newest_first
            .iter()
            .find(|e| e.kind == "question")
            .cloned();
    }
    if needs_proof {
        return events_newest_first
            .iter()
            .find(|e| e.kind == "proof_gate")
            .cloned();
    }
    if has_error {
        return events_newest_first
            .iter()
            .find(|e| e.kind == "error")
            .cloned();
    }

    events_newest_first
        .iter()
        .find(|e| e.kind != "heartbeat" && !is_runner_internal_message(&e.message))
        .or_else(|| events_newest_first.iter().find(|e| e.kind != "heartbeat"))
        .or_else(|| events_newest_first.first())
        .cloned()
}

fn normalize_job_id(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job id must not be empty"));
    }
    if !raw.starts_with("JOB-") {
        return Err(StoreError::InvalidInput("job id must start with JOB-"));
    }
    let digits = raw.trim_start_matches("JOB-");
    if digits.len() < 3 {
        return Err(StoreError::InvalidInput(
            "job id must have at least 3 digits",
        ));
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(StoreError::InvalidInput("job id digits must be [0-9]"));
    }
    Ok(raw.to_string())
}

fn normalize_job_status(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job.status must not be empty"));
    }
    let s = raw.to_ascii_uppercase();
    if !matches!(
        s.as_str(),
        "QUEUED" | "RUNNING" | "DONE" | "FAILED" | "CANCELED"
    ) {
        return Err(StoreError::InvalidInput("job.status is invalid"));
    }
    Ok(s)
}

fn normalize_job_priority(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job.priority must not be empty"));
    }
    let mut p = raw.to_ascii_uppercase();
    if p == "NORMAL" {
        p = "MEDIUM".to_string();
    }
    if !matches!(p.as_str(), "LOW" | "MEDIUM" | "HIGH") {
        return Err(StoreError::InvalidInput(
            "job.priority is invalid (expected LOW|MEDIUM|HIGH; synonym: normal->MEDIUM)",
        ));
    }
    Ok(p)
}

fn normalize_job_kind(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job.kind must not be empty"));
    }
    if raw.len() > MAX_JOB_KIND_LEN {
        return Err(StoreError::InvalidInput("job.kind is too long"));
    }
    Ok(raw.to_string())
}

fn normalize_job_title(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job.title must not be empty"));
    }
    Ok(raw.chars().take(MAX_JOB_TITLE_LEN).collect())
}

fn normalize_job_prompt(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job.prompt must not be empty"));
    }
    Ok(raw.chars().take(MAX_JOB_PROMPT_LEN).collect())
}

fn normalize_job_runner_id(raw: &str) -> Result<String, StoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(StoreError::InvalidInput("runner_id must not be empty"));
    }
    if trimmed.len() > MAX_JOB_RUNNER_LEN {
        return Err(StoreError::InvalidInput("runner_id is too long"));
    }
    Ok(trimmed.to_string())
}

fn normalize_job_summary(raw: Option<String>) -> Result<Option<String>, StoreError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.chars().take(MAX_JOB_SUMMARY_LEN).collect()))
}

fn normalize_event_kind(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("job_event.kind must not be empty"));
    }
    if raw.len() > MAX_EVENT_KIND_LEN {
        return Err(StoreError::InvalidInput("job_event.kind is too long"));
    }
    Ok(raw.to_string())
}

fn normalize_event_message(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput(
            "job_event.message must not be empty",
        ));
    }
    Ok(raw.chars().take(MAX_EVENT_MESSAGE_LEN).collect())
}

fn normalize_event_refs(mut refs: Vec<String>) -> Result<Vec<String>, StoreError> {
    if refs.len() > MAX_EVENT_REFS {
        return Err(StoreError::InvalidInput("job_event.refs exceeds max items"));
    }
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for r in refs.drain(..) {
        let trimmed = r.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_EVENT_REFS_ITEM_LEN {
            return Err(StoreError::InvalidInput("job_event.refs item too long"));
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

fn read_job_row(row: &rusqlite::Row<'_>, id: String) -> Result<JobRow, rusqlite::Error> {
    Ok(JobRow {
        id,
        revision: row.get(0)?,
        status: row.get(1)?,
        title: row.get(2)?,
        kind: row.get(3)?,
        priority: row.get(4)?,
        task_id: row.get(5)?,
        anchor_id: row.get(6)?,
        runner: row.get(7)?,
        claim_expires_at_ms: row.get(8)?,
        summary: row.get(9)?,
        created_at_ms: row.get(10)?,
        updated_at_ms: row.get(11)?,
        completed_at_ms: row.get(12)?,
    })
}

struct InsertJobEventTxArgs<'a> {
    ts_ms: i64,
    kind: &'a str,
    message: &'a str,
    percent: Option<i64>,
    refs: &'a [String],
    meta_json: Option<String>,
}

fn insert_job_event_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace: &str,
    job_id: &str,
    args: InsertJobEventTxArgs<'_>,
) -> Result<JobEventRow, StoreError> {
    let kind = normalize_event_kind(args.kind)?;
    let message = normalize_event_message(args.message)?;
    let percent = args.percent;
    let refs = normalize_event_refs(args.refs.to_vec())?;
    let refs_json = super::anchors::encode_json_string_list(&refs);
    let meta_json = args.meta_json.clone();

    // Noise control: long-running runners may emit frequent `heartbeat` events.
    //
    // To keep storage bounded and daily views low-noise, coalesce heartbeats by updating the most
    // recent heartbeat event in-place (when the last event is also a heartbeat).
    if kind == "heartbeat" {
        let last: Option<(i64, String)> = tx
            .query_row(
                r#"
                SELECT seq, kind
                FROM job_events
                WHERE workspace=?1 AND job_id=?2
                ORDER BY seq DESC
                LIMIT 1
                "#,
                params![workspace, job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((seq, last_kind)) = last
            && last_kind == "heartbeat"
        {
            tx.execute(
                r#"
                UPDATE job_events
                SET ts_ms=?1, message=?2, percent=?3, refs_json=?4, meta_json=?5
                WHERE seq=?6
                "#,
                params![args.ts_ms, message, percent, refs_json, args.meta_json, seq],
            )?;
            return Ok(JobEventRow {
                seq,
                job_id: job_id.to_string(),
                ts_ms: args.ts_ms,
                kind,
                message,
                percent,
                refs,
                meta_json,
            });
        }
    }

    tx.execute(
        r#"
        INSERT INTO job_events(workspace, job_id, ts_ms, kind, message, percent, refs_json, meta_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace,
            job_id,
            args.ts_ms,
            kind,
            message,
            percent,
            refs_json,
            args.meta_json
        ],
    )?;

    let seq = tx.last_insert_rowid();
    Ok(JobEventRow {
        seq,
        job_id: job_id.to_string(),
        ts_ms: args.ts_ms,
        kind,
        message,
        percent,
        refs,
        meta_json,
    })
}

impl SqliteStore {
    pub fn job_create(
        &mut self,
        workspace: &WorkspaceId,
        request: JobCreateRequest,
    ) -> Result<JobCreateResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let seq = next_counter_tx(&tx, workspace.as_str(), "job_seq")?;
        let id = format!("JOB-{seq:03}");

        let title = normalize_job_title(&request.title)?;
        let prompt = normalize_job_prompt(&request.prompt)?;
        let kind = normalize_job_kind(&request.kind)?;
        let priority = normalize_job_priority(&request.priority)?;

        let task_id = request
            .task_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let anchor_id = match request
            .anchor_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(raw) => {
                let normalized = super::anchors::normalize_anchor_id(raw)?;
                // Auto-create minimal anchor so jobs can always be listed/opened by meaning.
                super::anchors::ensure_anchor_exists_tx(
                    &tx,
                    workspace.as_str(),
                    &normalized,
                    now_ms,
                )?;
                Some(normalized)
            }
            None => None,
        };

        tx.execute(
            r#"
            INSERT INTO jobs(
              workspace, id, revision, status, title, kind, priority, task_id, anchor_id, runner,
              claim_expires_at_ms, prompt, summary, meta_json, created_at_ms, updated_at_ms, completed_at_ms
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                workspace.as_str(),
                id.as_str(),
                0i64,
                "QUEUED",
                title,
                kind,
                priority,
                task_id,
                anchor_id,
                Option::<String>::None,
                Option::<i64>::None,
                prompt,
                Option::<String>::None,
                request.meta_json,
                now_ms,
                now_ms,
                Option::<i64>::None,
            ],
        )?;

        let created_event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: "created",
                message: "created",
                percent: None,
                refs: &[],
                meta_json: None,
            },
        )?;

        let job = JobRow {
            id: id.clone(),
            revision: 0,
            status: "QUEUED".to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            priority: priority.to_string(),
            task_id,
            anchor_id,
            runner: None,
            claim_expires_at_ms: None,
            summary: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            completed_at_ms: None,
        };

        tx.commit()?;

        Ok(JobCreateResult { job, created_event })
    }

    pub fn jobs_list(
        &mut self,
        workspace: &WorkspaceId,
        request: JobsListRequest,
    ) -> Result<JobsListResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_LIST_LIMIT);
        let tx = self.conn.transaction()?;

        let status = match request
            .status
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(v) => Some(normalize_job_status(v)?),
            None => None,
        };
        let task_id = request
            .task_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let anchor_id = match request
            .anchor_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(raw) => Some(super::anchors::normalize_anchor_id(raw)?),
            None => None,
        };

        let mut jobs = Vec::<JobRow>::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT
                  revision,
                  status,
                  title,
                  kind,
                  priority,
                  task_id,
                  anchor_id,
                  runner,
                  claim_expires_at_ms,
                  summary,
                  created_at_ms,
                  updated_at_ms,
                  completed_at_ms,
                  id
                FROM jobs
                WHERE workspace=?1
                  AND (?2 IS NULL OR status=?2)
                  AND (?3 IS NULL OR task_id=?3)
                  AND (?4 IS NULL OR anchor_id=?4)
                ORDER BY updated_at_ms DESC, id ASC
                LIMIT ?5
                "#,
            )?;

            let mut rows = stmt.query(params![
                workspace.as_str(),
                status.as_deref(),
                task_id.as_deref(),
                anchor_id.as_deref(),
                (limit + 1) as i64
            ])?;

            while let Some(row) = rows.next()? {
                let id: String = row.get(13)?;
                let job = read_job_row(row, id)?;
                jobs.push(job);
            }
        }

        let has_more = jobs.len() > limit;
        if has_more {
            jobs.truncate(limit);
        }

        tx.commit()?;
        Ok(JobsListResult { jobs, has_more })
    }

    pub fn jobs_status_counts(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<JobsStatusCounts, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              COALESCE(SUM(CASE WHEN status='RUNNING' THEN 1 ELSE 0 END), 0) AS running,
              COALESCE(SUM(CASE WHEN status='QUEUED' THEN 1 ELSE 0 END), 0) AS queued
            FROM jobs
            WHERE workspace=?1
            "#,
        )?;
        let (running, queued) = stmt.query_row(params![workspace.as_str()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(JobsStatusCounts {
            running: running.max(0) as u64,
            queued: queued.max(0) as u64,
        })
    }

    pub fn jobs_radar(
        &mut self,
        workspace: &WorkspaceId,
        request: JobsRadarRequest,
    ) -> Result<JobsRadarResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_LIST_LIMIT);
        // Inbox UX: prioritize attention-worthy jobs (needs_manager / error / stale) even when
        // there are many active jobs. To avoid missing the “stuck” tail, we scan a bounded
        // superset and then sort deterministically in-memory.
        let scan_limit = limit.saturating_mul(4).clamp(limit, MAX_LIST_LIMIT);
        let tx = self.conn.transaction()?;

        let status = match request
            .status
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(v) => Some(normalize_job_status(v)?),
            None => None,
        };
        let task_id = request
            .task_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let anchor_id = match request
            .anchor_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(raw) => Some(super::anchors::normalize_anchor_id(raw)?),
            None => None,
        };
        let now_ms = now_ms();

        let mut jobs = Vec::<JobRow>::new();
        {
            // When status is omitted, radar defaults to *active* jobs only.
            // We keep the query deterministic and index-friendly.
            let mut stmt = tx.prepare(
                r#"
                SELECT
                  revision,
                  status,
                  title,
                  kind,
                  priority,
                  task_id,
                  anchor_id,
                  runner,
                  claim_expires_at_ms,
                  summary,
                  created_at_ms,
                  updated_at_ms,
                  completed_at_ms,
                  id
                FROM jobs
                WHERE workspace=?1
                  AND (
                    (?2 IS NOT NULL AND status=?2)
                    OR
                    (?2 IS NULL AND status IN ('RUNNING','QUEUED'))
                  )
                  AND (?3 IS NULL OR task_id=?3)
                  AND (?4 IS NULL OR anchor_id=?4)
                ORDER BY updated_at_ms DESC, id ASC
                LIMIT ?5
                "#,
            )?;

            let mut rows = stmt.query(params![
                workspace.as_str(),
                status.as_deref(),
                task_id.as_deref(),
                anchor_id.as_deref(),
                (scan_limit + 1) as i64
            ])?;

            while let Some(row) = rows.next()? {
                let id: String = row.get(13)?;
                let job = read_job_row(row, id)?;
                jobs.push(job);
            }
        }

        let scan_has_more = jobs.len() > scan_limit;
        if scan_has_more {
            jobs.truncate(scan_limit);
        }

        // Batch query: fetch up to MAX_RADAR_SCAN_EVENTS per job in a single SQL
        // call instead of N separate queries (N+1 → 2 total).
        let mut events_by_job = std::collections::HashMap::<String, Vec<JobEventRow>>::new();
        if !jobs.is_empty() {
            let placeholders: String = jobs
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 3))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
                SELECT job_id, seq, ts_ms, kind, message, percent, refs_json, meta_json
                FROM (
                    SELECT job_id, seq, ts_ms, kind, message, percent, refs_json, meta_json,
                           ROW_NUMBER() OVER (PARTITION BY job_id ORDER BY seq DESC) AS rn
                    FROM job_events
                    WHERE workspace = ?1 AND job_id IN ({placeholders})
                )
                WHERE rn <= ?2
                ORDER BY job_id, seq DESC
                "#,
            );
            let mut sql_params = Vec::<rusqlite::types::Value>::new();
            sql_params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
            sql_params.push(rusqlite::types::Value::Integer(
                MAX_RADAR_SCAN_EVENTS as i64,
            ));
            for job in &jobs {
                sql_params.push(rusqlite::types::Value::Text(job.id.clone()));
            }
            let mut stmt = tx.prepare(&sql)?;
            let mut ev_rows = stmt.query(params_from_iter(sql_params))?;
            while let Some(row) = ev_rows.next()? {
                let job_id: String = row.get(0)?;
                let seq: i64 = row.get(1)?;
                let ts_ms: i64 = row.get(2)?;
                let kind: String = row.get(3)?;
                let message: String = row.get(4)?;
                let percent: Option<i64> = row.get(5)?;
                let refs_json: Option<String> = row.get(6)?;
                let meta_json: Option<String> = row.get(7)?;
                let refs = super::anchors::decode_json_string_list(refs_json)?;
                events_by_job
                    .entry(job_id.clone())
                    .or_default()
                    .push(JobEventRow {
                        seq,
                        job_id,
                        ts_ms,
                        kind,
                        message,
                        percent,
                        refs,
                        meta_json,
                    });
            }
        }

        let mut rows = Vec::<JobRadarRow>::new();
        for job in jobs {
            let events = events_by_job.remove(&job.id).unwrap_or_default();

            let last_question_seq = events.iter().find(|e| e.kind == "question").map(|e| e.seq);
            let last_manager_seq = events.iter().find(|e| e.kind == "manager").map(|e| e.seq);
            let last_manager_proof_seq = events
                .iter()
                .find(|e| e.kind == "manager" && !e.refs.is_empty())
                .map(|e| e.seq);
            let last_error_seq = events.iter().find(|e| e.kind == "error").map(|e| e.seq);
            let last_proof_gate_seq = events
                .iter()
                .find(|e| e.kind == "proof_gate")
                .map(|e| e.seq);
            let last_checkpoint_seq = events
                .iter()
                .find(|e| e.kind == "checkpoint")
                .map(|e| e.seq);
            let last_checkpoint_ts_ms = events
                .iter()
                .find(|e| e.kind == "checkpoint")
                .map(|e| e.ts_ms);

            rows.push(JobRadarRow {
                job,
                last_event: pick_last_meaningful_job_event(&events),
                last_question_seq,
                last_manager_seq,
                last_manager_proof_seq,
                last_error_seq,
                last_proof_gate_seq,
                last_checkpoint_seq,
                last_checkpoint_ts_ms,
            });
        }

        // Deterministic attention-first ordering for the manager inbox.
        // Order by: error (!), needs_manager (?), stale (~), then RUNNING before QUEUED,
        // then newest-first, then id ASC.
        rows.sort_by(|a, b| {
            let a_needs_manager = a.last_question_seq.unwrap_or(0)
                > a.last_manager_seq.unwrap_or(0)
                && (a.job.status == "RUNNING" || a.job.status == "QUEUED");
            let b_needs_manager = b.last_question_seq.unwrap_or(0)
                > b.last_manager_seq.unwrap_or(0)
                && (b.job.status == "RUNNING" || b.job.status == "QUEUED");

            let a_has_error = a.last_error_seq.unwrap_or(0) > a.last_checkpoint_seq.unwrap_or(0)
                && a.job.status == "RUNNING";
            let b_has_error = b.last_error_seq.unwrap_or(0) > b.last_checkpoint_seq.unwrap_or(0)
                && b.job.status == "RUNNING";

            let a_needs_proof = a.last_proof_gate_seq.unwrap_or(0)
                > a.last_checkpoint_seq
                    .unwrap_or(0)
                    .max(a.last_manager_proof_seq.unwrap_or(0))
                && a.job.status == "RUNNING";
            let b_needs_proof = b.last_proof_gate_seq.unwrap_or(0)
                > b.last_checkpoint_seq
                    .unwrap_or(0)
                    .max(b.last_manager_proof_seq.unwrap_or(0))
                && b.job.status == "RUNNING";

            let a_stale = a.job.status == "RUNNING"
                && a.job
                    .claim_expires_at_ms
                    .map(|v| v <= now_ms)
                    .unwrap_or(true);
            let b_stale = b.job.status == "RUNNING"
                && b.job
                    .claim_expires_at_ms
                    .map(|v| v <= now_ms)
                    .unwrap_or(true);

            let a_status_rank = if a.job.status == "RUNNING" {
                0u8
            } else if a.job.status == "QUEUED" {
                1u8
            } else {
                2u8
            };
            let b_status_rank = if b.job.status == "RUNNING" {
                0u8
            } else if b.job.status == "QUEUED" {
                1u8
            } else {
                2u8
            };

            // Note: use explicit ordering, avoid floats/unstable hashes.
            (b_has_error as u8)
                .cmp(&(a_has_error as u8))
                .then((b_needs_manager as u8).cmp(&(a_needs_manager as u8)))
                .then((b_needs_proof as u8).cmp(&(a_needs_proof as u8)))
                .then((b_stale as u8).cmp(&(a_stale as u8)))
                .then(a_status_rank.cmp(&b_status_rank))
                .then(b.job.updated_at_ms.cmp(&a.job.updated_at_ms))
                .then(a.job.id.cmp(&b.job.id))
        });

        let mut has_more = scan_has_more;
        if rows.len() > limit {
            has_more = true;
            rows.truncate(limit);
        }

        tx.commit()?;
        Ok(JobsRadarResult { rows, has_more })
    }

    pub fn job_get(
        &mut self,
        workspace: &WorkspaceId,
        request: JobGetRequest,
    ) -> Result<Option<JobRow>, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let tx = self.conn.transaction()?;
        let row: Option<JobRow> = tx
            .query_row(
                r#"
                SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
                FROM jobs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id.as_str()],
                |row| read_job_row(row, id.clone()),
            )
            .optional()?;
        tx.commit()?;
        Ok(row)
    }

    pub fn job_open(
        &mut self,
        workspace: &WorkspaceId,
        request: JobOpenRequest,
    ) -> Result<JobOpenResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let max_events = request.max_events.clamp(0, MAX_OPEN_EVENTS);
        let include_prompt = request.include_prompt;
        let include_meta = request.include_meta;
        let include_events = request.include_events;
        let before_seq = match request.before_seq {
            Some(v) if v <= 0 => return Err(StoreError::InvalidInput("before_seq must be > 0")),
            Some(v) => Some(v),
            None => None,
        };

        let tx = self.conn.transaction()?;

        let row: Option<(JobRow, Option<String>, Option<String>)> = tx
            .query_row(
                r#"
                SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms, prompt, meta_json
                FROM jobs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id.as_str()],
                |row| {
                    let job = read_job_row(row, id.clone())?;
                    let prompt: Option<String> = row.get(13)?;
                    let meta_json: Option<String> = row.get(14)?;
                    Ok((job, prompt, meta_json))
                },
            )
            .optional()?;
        let Some((job, prompt, meta_json)) = row else {
            return Err(StoreError::UnknownId);
        };

        let prompt = if include_prompt { prompt } else { None };
        let meta_json = if include_meta { meta_json } else { None };

        let mut events = Vec::<JobEventRow>::new();
        let mut has_more_events = false;
        if include_events && max_events > 0 {
            if let Some(before_seq) = before_seq {
                let mut stmt = tx.prepare(
                    r#"
                    SELECT seq, ts_ms, kind, message, percent, refs_json, meta_json
                    FROM job_events
                    WHERE workspace=?1 AND job_id=?2 AND seq < ?3
                    ORDER BY seq DESC
                    LIMIT ?4
                    "#,
                )?;
                let mut rows = stmt.query(params![
                    workspace.as_str(),
                    id.as_str(),
                    before_seq,
                    (max_events + 1) as i64
                ])?;
                while let Some(row) = rows.next()? {
                    let seq: i64 = row.get(0)?;
                    let ts_ms: i64 = row.get(1)?;
                    let kind: String = row.get(2)?;
                    let message: String = row.get(3)?;
                    let percent: Option<i64> = row.get(4)?;
                    let refs_json: Option<String> = row.get(5)?;
                    let meta_json: Option<String> = row.get(6)?;
                    let refs = super::anchors::decode_json_string_list(refs_json)?;
                    events.push(JobEventRow {
                        seq,
                        job_id: id.clone(),
                        ts_ms,
                        kind,
                        message,
                        percent,
                        refs,
                        meta_json,
                    });
                }
            } else {
                let mut stmt = tx.prepare(
                    r#"
                    SELECT seq, ts_ms, kind, message, percent, refs_json, meta_json
                    FROM job_events
                    WHERE workspace=?1 AND job_id=?2
                    ORDER BY seq DESC
                    LIMIT ?3
                    "#,
                )?;
                let mut rows = stmt.query(params![
                    workspace.as_str(),
                    id.as_str(),
                    (max_events + 1) as i64
                ])?;
                while let Some(row) = rows.next()? {
                    let seq: i64 = row.get(0)?;
                    let ts_ms: i64 = row.get(1)?;
                    let kind: String = row.get(2)?;
                    let message: String = row.get(3)?;
                    let percent: Option<i64> = row.get(4)?;
                    let refs_json: Option<String> = row.get(5)?;
                    let meta_json: Option<String> = row.get(6)?;
                    let refs = super::anchors::decode_json_string_list(refs_json)?;
                    events.push(JobEventRow {
                        seq,
                        job_id: id.clone(),
                        ts_ms,
                        kind,
                        message,
                        percent,
                        refs,
                        meta_json,
                    });
                }
            }
            has_more_events = events.len() > max_events;
            if has_more_events {
                events.truncate(max_events);
            }
        }

        tx.commit()?;

        Ok(JobOpenResult {
            job,
            prompt,
            meta_json,
            events,
            has_more_events,
        })
    }

    pub fn job_event_get(
        &mut self,
        workspace: &WorkspaceId,
        request: JobEventGetRequest,
    ) -> Result<Option<JobEventRow>, StoreError> {
        let job_id = normalize_job_id(&request.job_id)?;
        let seq = request.seq;
        if seq <= 0 {
            return Err(StoreError::InvalidInput("job_event.seq must be > 0"));
        }

        let tx = self.conn.transaction()?;

        type JobEventGetRow = (
            i64,
            i64,
            String,
            String,
            Option<i64>,
            Option<String>,
            Option<String>,
        );

        let row: Option<JobEventGetRow> = tx
            .query_row(
                r#"
                SELECT seq, ts_ms, kind, message, percent, refs_json, meta_json
                FROM job_events
                WHERE workspace=?1 AND job_id=?2 AND seq=?3
                "#,
                params![workspace.as_str(), job_id.as_str(), seq],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;

        let out = if let Some((seq, ts_ms, kind, message, percent, refs_json, meta_json)) = row {
            let refs = super::anchors::decode_json_string_list(refs_json)?;
            Some(JobEventRow {
                seq,
                job_id: job_id.clone(),
                ts_ms,
                kind,
                message,
                percent,
                refs,
                meta_json,
            })
        } else {
            None
        };

        tx.commit()?;
        Ok(out)
    }

    pub fn job_events_tail(
        &mut self,
        workspace: &WorkspaceId,
        request: JobEventsTailRequest,
    ) -> Result<JobEventsTailResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let after_seq = request.after_seq;
        if after_seq < 0 {
            return Err(StoreError::InvalidInput("after_seq must be >= 0"));
        }
        let limit = request.limit.clamp(1, MAX_TAIL_EVENTS);

        let tx = self.conn.transaction()?;

        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::UnknownId);
        }

        let mut events = Vec::<JobEventRow>::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT seq, ts_ms, kind, message, percent, refs_json, meta_json
                FROM job_events
                WHERE workspace=?1 AND job_id=?2 AND seq > ?3
                ORDER BY seq ASC
                LIMIT ?4
                "#,
            )?;
            let mut rows = stmt.query(params![
                workspace.as_str(),
                id.as_str(),
                after_seq,
                (limit + 1) as i64
            ])?;

            while let Some(row) = rows.next()? {
                let seq: i64 = row.get(0)?;
                let ts_ms: i64 = row.get(1)?;
                let kind: String = row.get(2)?;
                let message: String = row.get(3)?;
                let percent: Option<i64> = row.get(4)?;
                let refs_json: Option<String> = row.get(5)?;
                let meta_json: Option<String> = row.get(6)?;
                let refs = super::anchors::decode_json_string_list(refs_json)?;
                events.push(JobEventRow {
                    seq,
                    job_id: id.clone(),
                    ts_ms,
                    kind,
                    message,
                    percent,
                    refs,
                    meta_json,
                });
            }
        }

        let has_more = events.len() > limit;
        if has_more {
            events.truncate(limit);
        }

        let next_after_seq = events.last().map(|e| e.seq).unwrap_or(after_seq);

        tx.commit()?;
        Ok(JobEventsTailResult {
            job_id: id,
            after_seq,
            next_after_seq,
            events,
            has_more,
        })
    }

    pub fn job_checkpoint_exists(
        &mut self,
        workspace: &WorkspaceId,
        request: JobCheckpointExistsRequest,
    ) -> Result<bool, StoreError> {
        let id = normalize_job_id(&request.id)?;

        // Fail closed on unknown ids to keep higher-level guardrails deterministic.
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::UnknownId);
        }

        let has_checkpoint: Option<i64> = self
            .conn
            .query_row(
                r#"
                SELECT 1
                FROM job_events
                WHERE workspace=?1 AND job_id=?2 AND kind='checkpoint'
                LIMIT 1
                "#,
                params![workspace.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        Ok(has_checkpoint.is_some())
    }

    pub fn job_claim(
        &mut self,
        workspace: &WorkspaceId,
        request: JobClaimRequest,
    ) -> Result<JobClaimResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let runner_id = normalize_job_runner_id(&request.runner_id)?;
        let allow_stale = request.allow_stale;
        let ttl_ms = request
            .lease_ttl_ms
            .clamp(MIN_JOB_CLAIM_TTL_MS, MAX_JOB_CLAIM_TTL_MS);
        let now_ms = now_ms();
        let claim_expires_at_ms = now_ms.saturating_add(ttl_ms.min(i64::MAX as u64) as i64);

        let tx = self.conn.transaction()?;

        let current: Option<(i64, String, Option<i64>, Option<String>)> = tx
            .query_row(
                "SELECT revision, status, claim_expires_at_ms, runner FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((revision, status, claim_expires_current, previous_runner_id)) = current else {
            return Err(StoreError::UnknownId);
        };

        let next_rev = revision + 1;
        let (event_kind, event_message) = if status == "QUEUED" {
            let changed = tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status='RUNNING', runner=?4, claim_expires_at_ms=?5, updated_at_ms=?6, completed_at_ms=NULL
                WHERE workspace=?1 AND id=?2 AND revision=?7 AND status='QUEUED'
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    runner_id,
                    claim_expires_at_ms,
                    now_ms,
                    revision
                ],
            )?;
            if changed != 1 {
                return Err(StoreError::JobNotClaimable {
                    job_id: id,
                    status: "QUEUED".to_string(),
                });
            }
            ("claimed", "claimed")
        } else if allow_stale && status == "RUNNING" {
            let expired = claim_expires_current.unwrap_or(0) <= now_ms;
            if !expired {
                return Err(StoreError::JobNotClaimable { job_id: id, status });
            }
            let changed = tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, runner=?4, claim_expires_at_ms=?5, updated_at_ms=?6, completed_at_ms=NULL
                WHERE workspace=?1 AND id=?2 AND revision=?7 AND status='RUNNING'
                  AND (claim_expires_at_ms IS NULL OR claim_expires_at_ms <= ?6)
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    runner_id,
                    claim_expires_at_ms,
                    now_ms,
                    revision
                ],
            )?;
            if changed != 1 {
                return Err(StoreError::JobNotClaimable {
                    job_id: id,
                    status: "RUNNING".to_string(),
                });
            }
            ("reclaimed", "reclaimed")
        } else {
            return Err(StoreError::JobNotClaimable { job_id: id, status });
        };

        let meta_json = if event_kind == "reclaimed" {
            let mut meta = JsonMap::<String, JsonValue>::new();
            if let Some(prev) = previous_runner_id
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                meta.insert(
                    "previous_runner_id".to_string(),
                    JsonValue::String(prev.to_string()),
                );
            }
            meta.insert(
                "reason".to_string(),
                JsonValue::String("ttl_expired".to_string()),
            );
            Some(JsonValue::Object(meta).to_string())
        } else {
            None
        };

        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: event_kind,
                message: event_message,
                percent: None,
                refs: &[],
                meta_json,
            },
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobClaimResult { job, event })
    }

    pub fn job_report(
        &mut self,
        workspace: &WorkspaceId,
        request: JobReportRequest,
    ) -> Result<JobReportResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let runner_id = normalize_job_runner_id(&request.runner_id)?;
        if request.claim_revision < 0 {
            return Err(StoreError::InvalidInput("claim_revision must be >= 0"));
        }
        let now_ms = now_ms();
        let ttl_ms = request
            .lease_ttl_ms
            .clamp(MIN_JOB_CLAIM_TTL_MS, MAX_JOB_CLAIM_TTL_MS);
        let claim_expires_at_ms = now_ms.saturating_add(ttl_ms.min(i64::MAX as u64) as i64);

        let kind = normalize_event_kind(&request.kind)?;
        let message = normalize_event_message(&request.message)?;
        let refs = normalize_event_refs(request.refs)?;
        let meta_json = request.meta_json.clone();

        let tx = self.conn.transaction()?;

        let current: Option<(i64, String, Option<String>)> = tx
            .query_row(
                "SELECT revision, status, runner FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((revision, status, runner)) = current else {
            return Err(StoreError::UnknownId);
        };
        if status != "RUNNING" {
            return Err(StoreError::JobNotRunning { job_id: id, status });
        }
        if runner.as_deref() != Some(runner_id.as_str()) || revision != request.claim_revision {
            return Err(StoreError::JobClaimMismatch {
                job_id: id,
                expected_runner_id: runner,
                actual_runner_id: runner_id,
                expected_revision: revision,
                actual_revision: request.claim_revision,
            });
        }

        let changed = if meta_json.is_some() {
            tx.execute(
                r#"
                UPDATE jobs
                SET updated_at_ms=?5, claim_expires_at_ms=?6, meta_json=?7
                WHERE workspace=?1 AND id=?2 AND status='RUNNING' AND revision=?3 AND runner=?4
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    request.claim_revision,
                    runner_id.as_str(),
                    now_ms,
                    claim_expires_at_ms,
                    meta_json
                ],
            )?
        } else {
            tx.execute(
                r#"
                UPDATE jobs
                SET updated_at_ms=?5, claim_expires_at_ms=?6
                WHERE workspace=?1 AND id=?2 AND status='RUNNING' AND revision=?3 AND runner=?4
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    request.claim_revision,
                    runner_id.as_str(),
                    now_ms,
                    claim_expires_at_ms
                ],
            )?
        };
        if changed != 1 {
            let current: Option<(i64, String, Option<String>)> = tx
                .query_row(
                    "SELECT revision, status, runner FROM jobs WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((revision, status, runner)) = current else {
                return Err(StoreError::UnknownId);
            };
            if status != "RUNNING" {
                return Err(StoreError::JobNotRunning { job_id: id, status });
            }
            return Err(StoreError::JobClaimMismatch {
                job_id: id,
                expected_runner_id: runner,
                actual_runner_id: runner_id,
                expected_revision: revision,
                actual_revision: request.claim_revision,
            });
        }

        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: &kind,
                message: &message,
                percent: request.percent,
                refs: &refs,
                meta_json: request.meta_json,
            },
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobReportResult { job, event })
    }

    pub fn job_message(
        &mut self,
        workspace: &WorkspaceId,
        request: JobMessageRequest,
    ) -> Result<JobMessageResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let now_ms = now_ms();

        // Manager -> job communication is intentionally low-noise and short. Long context belongs
        // in stable refs (CARD-*, notes@seq, etc).
        let kind = "manager";
        let message = normalize_event_message(&request.message)?;
        let refs = normalize_event_refs(request.refs)?;

        let tx = self.conn.transaction()?;

        let status: Option<String> = tx
            .query_row(
                "SELECT status FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(status) = status else {
            return Err(StoreError::UnknownId);
        };
        if status != "QUEUED" && status != "RUNNING" {
            return Err(StoreError::JobNotMessageable { job_id: id, status });
        }

        tx.execute(
            "UPDATE jobs SET updated_at_ms=?3 WHERE workspace=?1 AND id=?2",
            params![workspace.as_str(), id.as_str(), now_ms],
        )?;

        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind,
                message: &message,
                percent: None,
                refs: &refs,
                meta_json: None,
            },
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobMessageResult { job, event })
    }

    pub fn job_complete(
        &mut self,
        workspace: &WorkspaceId,
        request: JobCompleteRequest,
    ) -> Result<JobCompleteResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let runner_id = normalize_job_runner_id(&request.runner_id)?;
        if request.claim_revision < 0 {
            return Err(StoreError::InvalidInput("claim_revision must be >= 0"));
        }
        let status = normalize_job_status(&request.status)?;
        if !matches!(status.as_str(), "DONE" | "FAILED" | "CANCELED") {
            return Err(StoreError::InvalidInput(
                "job_complete status must be terminal",
            ));
        }

        let summary = normalize_job_summary(request.summary)?;
        let refs = normalize_event_refs(request.refs)?;
        let meta_json = request.meta_json.clone();
        let now_ms = now_ms();

        let tx = self.conn.transaction()?;

        let current: Option<(i64, String, Option<String>)> = tx
            .query_row(
                "SELECT revision, status, runner FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((revision, current_status, runner)) = current else {
            return Err(StoreError::UnknownId);
        };
        if matches!(current_status.as_str(), "DONE" | "FAILED" | "CANCELED") {
            return Err(StoreError::JobAlreadyTerminal {
                job_id: id,
                status: current_status,
            });
        }
        if current_status != "RUNNING" {
            return Err(StoreError::JobNotRunning {
                job_id: id,
                status: current_status,
            });
        }
        if runner.as_deref() != Some(runner_id.as_str()) || revision != request.claim_revision {
            return Err(StoreError::JobClaimMismatch {
                job_id: id,
                expected_runner_id: runner,
                actual_runner_id: runner_id,
                expected_revision: revision,
                actual_revision: request.claim_revision,
            });
        }

        let next_rev = revision + 1;
        if meta_json.is_some() {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status=?4, summary=?5, meta_json=?6, updated_at_ms=?7, completed_at_ms=?8, claim_expires_at_ms=NULL
                WHERE workspace=?1 AND id=?2 AND status='RUNNING' AND revision=?9 AND runner=?10
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    status,
                    summary,
                    meta_json,
                    now_ms,
                    now_ms,
                    request.claim_revision,
                    runner_id.as_str(),
                ],
            )?;
        } else {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status=?4, summary=?5, updated_at_ms=?6, completed_at_ms=?7, claim_expires_at_ms=NULL
                WHERE workspace=?1 AND id=?2 AND status='RUNNING' AND revision=?8 AND runner=?9
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    status,
                    summary,
                    now_ms,
                    now_ms,
                    request.claim_revision,
                    runner_id.as_str(),
                ],
            )?;
        }

        let event_kind = match status.as_str() {
            "DONE" => "completed",
            "FAILED" => "failed",
            "CANCELED" => "canceled",
            _ => "completed",
        };
        let event_message = match status.as_str() {
            "DONE" => "done",
            "FAILED" => "failed",
            "CANCELED" => "canceled",
            _ => "done",
        };
        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: event_kind,
                message: event_message,
                percent: None,
                refs: &refs,
                meta_json: request.meta_json,
            },
        )?;

        // Self-heal: a completed job must never keep a runner "stuck" via runner_leases.active_job_id.
        // Clear any active lease that still points at this job (covers both the completing runner and
        // any inconsistent multi-runner scenarios).
        tx.execute(
            r#"
            UPDATE runner_leases
            SET active_job_id=NULL, updated_at_ms=?3
            WHERE workspace=?1 AND active_job_id=?2
            "#,
            params![workspace.as_str(), id.as_str(), now_ms],
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobCompleteResult { job, event })
    }

    pub fn job_cancel(
        &mut self,
        workspace: &WorkspaceId,
        request: JobCancelRequest,
    ) -> Result<JobCancelResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let now_ms = now_ms();
        let summary = normalize_job_summary(request.reason.clone())?;
        let refs = normalize_event_refs(request.refs)?;
        let meta_json = request.meta_json.clone();
        let force_running = request.force_running;
        let expected_revision = request.expected_revision;

        let tx = self.conn.transaction()?;

        let current: Option<(i64, String, Option<String>)> = tx
            .query_row(
                "SELECT revision, status, runner FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((revision, status, runner)) = current else {
            return Err(StoreError::UnknownId);
        };
        if matches!(status.as_str(), "DONE" | "FAILED" | "CANCELED") {
            return Err(StoreError::JobAlreadyTerminal { job_id: id, status });
        }
        if let Some(expected) = expected_revision {
            if expected < 0 {
                return Err(StoreError::InvalidInput("expected_revision must be >= 0"));
            }
            if revision != expected {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }
        if status == "RUNNING" && !force_running {
            return Err(StoreError::JobNotCancelable { job_id: id, status });
        }
        if status != "QUEUED" && status != "RUNNING" {
            return Err(StoreError::JobNotCancelable { job_id: id, status });
        }

        let next_rev = revision + 1;
        if meta_json.is_some() {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status='CANCELED',
                    runner=CASE WHEN status='QUEUED' THEN NULL ELSE runner END,
                    claim_expires_at_ms=NULL, summary=?4, meta_json=?5, updated_at_ms=?6, completed_at_ms=?7
                WHERE workspace=?1 AND id=?2 AND (status='QUEUED' OR status='RUNNING') AND revision=?8
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    summary,
                    meta_json,
                    now_ms,
                    now_ms,
                    revision
                ],
            )?;
        } else {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status='CANCELED',
                    runner=CASE WHEN status='QUEUED' THEN NULL ELSE runner END,
                    claim_expires_at_ms=NULL, summary=?4, updated_at_ms=?5, completed_at_ms=?6
                WHERE workspace=?1 AND id=?2 AND (status='QUEUED' OR status='RUNNING') AND revision=?7
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    summary,
                    now_ms,
                    now_ms,
                    revision
                ],
            )?;
        }

        // When a RUNNING job is force-canceled, clear any runner lease that still points at it.
        // (Even when runner_id is missing, active_job_id is enough to self-heal deterministically.)
        if status == "RUNNING" || runner.is_some() {
            tx.execute(
                r#"
                UPDATE runner_leases
                SET active_job_id=NULL, updated_at_ms=?3
                WHERE workspace=?1 AND active_job_id=?2
                "#,
                params![workspace.as_str(), id.as_str(), now_ms],
            )?;
        }

        let reason = request
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let message = if let Some(reason) = reason {
            normalize_event_message(&format!("canceled: {reason}"))?
        } else {
            "canceled".to_string()
        };

        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: "canceled",
                message: &message,
                percent: None,
                refs: &refs,
                meta_json: request.meta_json,
            },
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobCancelResult { job, event })
    }

    pub fn job_requeue(
        &mut self,
        workspace: &WorkspaceId,
        request: JobRequeueRequest,
    ) -> Result<JobRequeueResult, StoreError> {
        let id = normalize_job_id(&request.id)?;
        let refs = normalize_event_refs(request.refs)?;
        let meta_json = request.meta_json.clone();
        let now_ms = now_ms();

        let tx = self.conn.transaction()?;

        let current: Option<(i64, String)> = tx
            .query_row(
                "SELECT revision, status FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((revision, status)) = current else {
            return Err(StoreError::UnknownId);
        };
        if !matches!(status.as_str(), "DONE" | "FAILED" | "CANCELED") {
            return Err(StoreError::JobNotRequeueable { job_id: id, status });
        }

        let next_rev = revision + 1;
        if meta_json.is_some() {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status='QUEUED', runner=NULL, claim_expires_at_ms=NULL, summary=NULL, meta_json=?4, updated_at_ms=?5, completed_at_ms=NULL
                WHERE workspace=?1 AND id=?2
                "#,
                params![
                    workspace.as_str(),
                    id.as_str(),
                    next_rev,
                    meta_json,
                    now_ms
                ],
            )?;
        } else {
            tx.execute(
                r#"
                UPDATE jobs
                SET revision=?3, status='QUEUED', runner=NULL, claim_expires_at_ms=NULL, summary=NULL, updated_at_ms=?4, completed_at_ms=NULL
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id.as_str(), next_rev, now_ms],
            )?;
        }

        let reason = request
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let message = if let Some(reason) = reason {
            format!("requeued: {reason}")
        } else {
            "requeued".to_string()
        };

        let event = insert_job_event_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
            InsertJobEventTxArgs {
                ts_ms: now_ms,
                kind: "requeued",
                message: &message,
                percent: None,
                refs: &refs,
                meta_json: request.meta_json,
            },
        )?;

        let job: JobRow = tx.query_row(
            r#"
            SELECT revision, status, title, kind, priority, task_id, anchor_id, runner, claim_expires_at_ms, summary, created_at_ms, updated_at_ms, completed_at_ms
            FROM jobs
            WHERE workspace=?1 AND id=?2
            "#,
            params![workspace.as_str(), id.as_str()],
            |row| read_job_row(row, id.clone()),
        )?;

        tx.commit()?;
        Ok(JobRequeueResult { job, event })
    }
}
