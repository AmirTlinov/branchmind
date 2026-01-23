#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params_from_iter;
use rusqlite::{OptionalExtension, Transaction, params};

const MAP_ANCHOR_ID_PREFIX: &str = "a:";
const TASK_BRANCH_PREFIX: &str = "task/";
const TASK_BRANCH_TASK_PREFIX: &str = "task/TASK-";
const MAX_LIST_LIMIT: usize = 200;
const MAX_TASKS_LIMIT: usize = 50;
const MAX_TASK_ANCHORS_LIMIT: usize = 50;
const MAX_PLAN_TOP_ANCHORS_LIMIT: usize = 20;
const TAG_PINNED: &str = "pinned";
const TAG_VIS_CANON: &str = "v:canon";

fn extract_anchor_ids(tags: &[String]) -> Vec<String> {
    let mut out = std::collections::BTreeSet::<String>::new();
    for tag in tags {
        let raw = tag.trim();
        if raw.is_empty() {
            continue;
        }
        if !raw.starts_with(MAP_ANCHOR_ID_PREFIX) {
            continue;
        }
        // Anchor tags are best-effort: invalid tags are treated as normal tags and ignored by the
        // meaning-map index (no hard failure / no dead-ends).
        if let Ok(id) = crate::store::anchors::normalize_anchor_id(raw) {
            out.insert(id);
        }
    }
    out.into_iter().collect()
}

fn card_is_canon(card_type: &str, tags: &[String]) -> bool {
    if tags.iter().any(|t| t == TAG_PINNED || t == TAG_VIS_CANON) {
        return true;
    }
    matches!(
        card_type.trim().to_ascii_lowercase().as_str(),
        "decision" | "evidence" | "test"
    )
}

fn resolve_anchor_id_alias_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    anchor_id: &str,
) -> Result<String, StoreError> {
    // Alias redirects are stable and deterministic. We keep the anchor-links index canonical to
    // make queries cheaper and avoid duplicating history under both alias+canonical ids.
    let owner: Option<String> = tx
        .query_row(
            "SELECT anchor_id FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
            params![workspace, anchor_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(owner.unwrap_or_else(|| anchor_id.to_string()))
}

fn task_id_from_task_branch(branch: &str) -> Option<&str> {
    branch.strip_prefix(TASK_BRANCH_PREFIX).and_then(|rest| {
        if rest.starts_with("TASK-") && !rest.trim().is_empty() {
            Some(rest)
        } else {
            None
        }
    })
}

pub(in crate::store) struct UpsertAnchorLinksForCardTxArgs<'a> {
    pub branch: &'a str,
    pub graph_doc: &'a str,
    pub card_id: &'a str,
    pub card_type: &'a str,
    pub tags: &'a [String],
    pub now_ms: i64,
}

pub(in crate::store) fn upsert_anchor_links_for_card_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    args: UpsertAnchorLinksForCardTxArgs<'_>,
) -> Result<usize, StoreError> {
    let mut anchor_ids = extract_anchor_ids(args.tags);
    if anchor_ids.is_empty() {
        return Ok(0);
    }

    let is_canon = card_is_canon(args.card_type, args.tags);

    let mut touched = 0usize;
    for anchor_id in anchor_ids.drain(..) {
        let anchor_id = resolve_anchor_id_alias_tx(tx, workspace, &anchor_id)?;

        // Autoregister: when canonical artifacts (decisions/evidence/tests or explicitly v:canon/pinned)
        // reference an anchor, create a minimal anchor record if missing. This avoids “dead anchors”
        // that cannot be opened or listed after /compact or restart.
        if is_canon {
            crate::store::anchors::ensure_anchor_exists_tx(
                tx,
                workspace,
                anchor_id.as_str(),
                args.now_ms,
            )?;
        }

        let rows = tx.execute(
            r#"
            INSERT INTO anchor_links(workspace, anchor_id, branch, graph_doc, card_id, card_type, last_ts_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(workspace, anchor_id, branch, graph_doc, card_id) DO UPDATE SET
              card_type=excluded.card_type,
              last_ts_ms=excluded.last_ts_ms
            "#,
            params![
                workspace,
                anchor_id,
                args.branch,
                args.graph_doc,
                args.card_id,
                args.card_type,
                args.now_ms
            ],
        )?;
        touched = touched.saturating_add(rows.max(0) as usize);
    }

    Ok(touched)
}

impl SqliteStore {
    pub fn anchor_links_list(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorLinksListRequest,
    ) -> Result<AnchorLinksListResult, StoreError> {
        let anchor_id = crate::store::anchors::normalize_anchor_id(&request.anchor_id)?;
        let limit = request.limit.clamp(0, MAX_LIST_LIMIT);
        let query_limit = limit.saturating_add(1) as i64;

        let tx = self.conn.transaction()?;

        let mut stmt = tx.prepare(
            r#"
            SELECT anchor_id, branch, graph_doc, card_id, card_type, last_ts_ms
            FROM anchor_links
            WHERE workspace=?1 AND anchor_id=?2
            ORDER BY last_ts_ms DESC, card_id ASC, branch ASC, graph_doc ASC
            LIMIT ?3
            "#,
        )?;

        let mut rows = stmt.query(params![workspace.as_str(), anchor_id.as_str(), query_limit])?;
        let mut links = Vec::<AnchorLinkRow>::new();
        while let Some(row) = rows.next()? {
            links.push(AnchorLinkRow {
                anchor_id: row.get(0)?,
                branch: row.get(1)?,
                graph_doc: row.get(2)?,
                card_id: row.get(3)?,
                card_type: row.get(4)?,
                last_ts_ms: row.get(5)?,
            });
        }
        drop(rows);
        drop(stmt);

        let has_more = links.len() > limit;
        links.truncate(limit);

        tx.commit()?;
        Ok(AnchorLinksListResult { links, has_more })
    }

    pub fn anchor_links_list_any(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorLinksListAnyRequest,
    ) -> Result<AnchorLinksListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_LIST_LIMIT);
        let query_limit = limit.saturating_add(1) as i64;

        let mut anchor_ids = Vec::<String>::new();
        for raw in request.anchor_ids.into_iter() {
            anchor_ids.push(crate::store::anchors::normalize_anchor_id(&raw)?);
        }
        anchor_ids.sort();
        anchor_ids.dedup();
        if anchor_ids.is_empty() {
            return Ok(AnchorLinksListResult {
                links: Vec::new(),
                has_more: false,
            });
        }

        let tx = self.conn.transaction()?;

        let mut placeholders = String::new();
        for idx in 0..anchor_ids.len() {
            if idx > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }

        let sql = format!(
            r#"
            SELECT anchor_id, branch, graph_doc, card_id, card_type, last_ts_ms
            FROM anchor_links
            WHERE workspace=? AND anchor_id IN ({placeholders})
            ORDER BY last_ts_ms DESC, card_id ASC, branch ASC, graph_doc ASC, anchor_id ASC
            LIMIT ?
            "#,
            placeholders = placeholders,
        );

        let mut params = Vec::<rusqlite::types::Value>::new();
        params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
        for id in &anchor_ids {
            params.push(rusqlite::types::Value::Text(id.clone()));
        }
        params.push(rusqlite::types::Value::Integer(query_limit));

        let mut stmt = tx.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(params))?;

        let mut links = Vec::<AnchorLinkRow>::new();
        while let Some(row) = rows.next()? {
            links.push(AnchorLinkRow {
                anchor_id: row.get(0)?,
                branch: row.get(1)?,
                graph_doc: row.get(2)?,
                card_id: row.get(3)?,
                card_type: row.get(4)?,
                last_ts_ms: row.get(5)?,
            });
        }
        drop(rows);
        drop(stmt);

        let has_more = links.len() > limit;
        links.truncate(limit);

        tx.commit()?;
        Ok(AnchorLinksListResult { links, has_more })
    }

    pub fn anchor_tasks_list_any(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorTasksListAnyRequest,
    ) -> Result<AnchorTasksListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_TASKS_LIMIT);
        if limit == 0 {
            return Ok(AnchorTasksListResult {
                tasks: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;

        let mut anchor_ids = Vec::<String>::new();
        for raw in request.anchor_ids.into_iter() {
            anchor_ids.push(crate::store::anchors::normalize_anchor_id(&raw)?);
        }
        anchor_ids.sort();
        anchor_ids.dedup();
        if anchor_ids.is_empty() {
            return Ok(AnchorTasksListResult {
                tasks: Vec::new(),
                has_more: false,
            });
        }

        let tx = self.conn.transaction()?;

        let mut placeholders = String::new();
        for idx in 0..anchor_ids.len() {
            if idx > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }

        let sql = format!(
            r#"
            SELECT branch, MAX(last_ts_ms) AS max_ts_ms
            FROM anchor_links
            WHERE workspace=? AND anchor_id IN ({placeholders}) AND branch LIKE '{task_prefix}%'
            GROUP BY branch
            ORDER BY max_ts_ms DESC, branch ASC
            LIMIT ?
            "#,
            placeholders = placeholders,
            task_prefix = TASK_BRANCH_TASK_PREFIX,
        );

        let mut params = Vec::<rusqlite::types::Value>::new();
        params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
        for id in &anchor_ids {
            params.push(rusqlite::types::Value::Text(id.clone()));
        }
        params.push(rusqlite::types::Value::Integer(query_limit));

        let mut stmt = tx.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(params))?;

        let mut branches = Vec::<(String, i64)>::new();
        while let Some(row) = rows.next()? {
            branches.push((row.get(0)?, row.get(1)?));
        }
        drop(rows);
        drop(stmt);

        let has_more = branches.len() > limit;
        branches.truncate(limit);

        let mut task_stmt = tx.prepare(
            r#"
            SELECT title, status
            FROM tasks
            WHERE workspace=?1 AND id=?2
            "#,
        )?;

        let mut tasks = Vec::<AnchorTaskHit>::new();
        for (branch, last_ts_ms) in branches {
            let Some(task_id) = task_id_from_task_branch(&branch) else {
                continue;
            };
            let task_id = task_id.to_string();

            let task_row = task_stmt
                .query_row(params![workspace.as_str(), task_id.as_str()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .optional()?;
            let (title, status) = match task_row {
                Some((title, status)) => (Some(title), Some(status)),
                None => (None, None),
            };

            tasks.push(AnchorTaskHit {
                task_id,
                title,
                status,
                last_ts_ms,
            });
        }
        drop(task_stmt);

        tx.commit()?;
        Ok(AnchorTasksListResult { tasks, has_more })
    }

    pub fn task_anchors_list(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskAnchorsListRequest,
    ) -> Result<TaskAnchorsListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_TASK_ANCHORS_LIMIT);
        if limit == 0 {
            return Ok(TaskAnchorsListResult {
                anchors: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;

        let task_id = request.task_id.trim().to_string();
        if task_id.is_empty() {
            return Err(StoreError::InvalidInput("task_id must not be empty"));
        }
        if !task_id.starts_with("TASK-")
            || !task_id
                .trim_start_matches("TASK-")
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return Err(StoreError::InvalidInput(
                "task_id must start with TASK- and contain only digits",
            ));
        }

        let branch = format!("{TASK_BRANCH_PREFIX}{task_id}");

        let tx = self.conn.transaction()?;

        let mut stmt = tx.prepare(
            r#"
            SELECT anchor_id, MAX(last_ts_ms) AS max_ts_ms
            FROM anchor_links
            WHERE workspace=?1 AND branch=?2
            GROUP BY anchor_id
            ORDER BY max_ts_ms DESC, anchor_id ASC
            LIMIT ?3
            "#,
        )?;

        let mut rows = stmt.query(params![workspace.as_str(), branch.as_str(), query_limit])?;
        let mut anchors = Vec::<TaskAnchorHit>::new();
        while let Some(row) = rows.next()? {
            anchors.push(TaskAnchorHit {
                anchor_id: row.get(0)?,
                last_ts_ms: row.get(1)?,
            });
        }
        drop(rows);
        drop(stmt);

        let has_more = anchors.len() > limit;
        anchors.truncate(limit);

        tx.commit()?;
        Ok(TaskAnchorsListResult { anchors, has_more })
    }

    pub fn plan_anchors_coverage(
        &mut self,
        workspace: &WorkspaceId,
        request: PlanAnchorsCoverageRequest,
    ) -> Result<PlanAnchorsCoverageResult, StoreError> {
        let plan_id = request.plan_id.trim().to_string();
        if plan_id.is_empty() {
            return Err(StoreError::InvalidInput("plan_id must not be empty"));
        }
        if !plan_id.starts_with("PLAN-")
            || !plan_id
                .trim_start_matches("PLAN-")
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return Err(StoreError::InvalidInput(
                "plan_id must start with PLAN- and contain only digits",
            ));
        }

        let top_limit = request
            .top_anchors_limit
            .clamp(0, MAX_PLAN_TOP_ANCHORS_LIMIT);
        let query_limit = top_limit.saturating_add(1) as i64;

        let tx = self.conn.transaction()?;

        let active_total: u64 = tx
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE workspace=?1 AND parent_plan_id=?2 AND status='ACTIVE'",
                params![workspace.as_str(), plan_id.as_str()],
                |row| row.get::<_, i64>(0),
            )?
            .max(0) as u64;

        if active_total == 0 || top_limit == 0 {
            // Even when we skip top anchors, we still compute missing count deterministically.
            let missing: u64 = tx
                .query_row(
                    r#"
                    SELECT COUNT(*)
                    FROM tasks t
                    WHERE t.workspace=?1 AND t.parent_plan_id=?2 AND t.status='ACTIVE'
                      AND NOT EXISTS (
                        SELECT 1
                        FROM anchor_links al
                        WHERE al.workspace=t.workspace AND al.branch=('task/' || t.id)
                        LIMIT 1
                      )
                    "#,
                    params![workspace.as_str(), plan_id.as_str()],
                    |row| row.get::<_, i64>(0),
                )?
                .max(0) as u64;
            tx.commit()?;
            return Ok(PlanAnchorsCoverageResult {
                active_total,
                active_missing_anchor: missing,
                top_anchors: Vec::new(),
            });
        }

        let active_missing_anchor: u64 = tx
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM tasks t
                WHERE t.workspace=?1 AND t.parent_plan_id=?2 AND t.status='ACTIVE'
                  AND NOT EXISTS (
                    SELECT 1
                    FROM anchor_links al
                    WHERE al.workspace=t.workspace AND al.branch=('task/' || t.id)
                    LIMIT 1
                  )
                "#,
                params![workspace.as_str(), plan_id.as_str()],
                |row| row.get::<_, i64>(0),
            )?
            .max(0) as u64;

        let mut stmt = tx.prepare(
            r#"
            WITH active_tasks AS (
              SELECT ('task/' || id) AS branch
              FROM tasks
              WHERE workspace=?1 AND parent_plan_id=?2 AND status='ACTIVE'
            ),
            per_task_anchor AS (
              SELECT al.anchor_id, al.branch, MAX(al.last_ts_ms) AS max_ts_ms
              FROM anchor_links al
              JOIN active_tasks at ON at.branch = al.branch
              WHERE al.workspace=?1 AND al.anchor_id LIKE 'a:%'
              GROUP BY al.anchor_id, al.branch
            ),
            per_anchor AS (
              SELECT anchor_id, MAX(max_ts_ms) AS max_ts_ms, COUNT(*) AS task_count
              FROM per_task_anchor
              GROUP BY anchor_id
            )
            SELECT anchor_id, max_ts_ms, task_count
            FROM per_anchor
            ORDER BY max_ts_ms DESC, task_count DESC, anchor_id ASC
            LIMIT ?3
            "#,
        )?;

        let mut rows = stmt.query(params![workspace.as_str(), plan_id.as_str(), query_limit])?;
        let mut hits = Vec::<PlanAnchorHit>::new();
        while let Some(row) = rows.next()? {
            hits.push(PlanAnchorHit {
                anchor_id: row.get(0)?,
                last_ts_ms: row.get(1)?,
                task_count: (row.get::<_, i64>(2)?).max(0) as u64,
            });
        }
        drop(rows);
        drop(stmt);

        let has_more = hits.len() > top_limit;
        hits.truncate(top_limit);
        if has_more {
            // Intentionally ignore: callers can re-query with a larger limit if they need the full distribution.
        }

        tx.commit()?;
        Ok(PlanAnchorsCoverageResult {
            active_total,
            active_missing_anchor,
            top_anchors: hits,
        })
    }

    pub fn anchor_link_exists(
        &mut self,
        workspace: &WorkspaceId,
        anchor_id: &str,
    ) -> Result<bool, StoreError> {
        let anchor_id = crate::store::anchors::normalize_anchor_id(anchor_id)?;
        let tx = self.conn.transaction()?;
        let exists = tx
            .query_row(
                "SELECT 1 FROM anchor_links WHERE workspace=?1 AND anchor_id=?2 LIMIT 1",
                params![workspace.as_str(), anchor_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        tx.commit()?;
        Ok(exists)
    }
}
