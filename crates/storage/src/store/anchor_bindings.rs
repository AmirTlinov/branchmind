#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{Transaction, params, params_from_iter};

const MAX_LOOKUP_LIMIT: usize = 200;
const MAX_INDEX_LIST_LIMIT: usize = 500;

pub(in crate::store) fn anchor_bindings_list_for_anchor_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    anchor_id: &str,
) -> Result<Vec<AnchorBindingRow>, StoreError> {
    let mut out = Vec::<AnchorBindingRow>::new();
    let mut stmt = match tx.prepare(
        r#"
        SELECT kind, repo_rel, created_at_ms, updated_at_ms
        FROM anchor_bindings
        WHERE workspace=?1 AND anchor_id=?2
        ORDER BY kind ASC, repo_rel ASC
        "#,
    ) {
        Ok(stmt) => stmt,
        Err(err) if is_missing_table(&err, "anchor_bindings") => return Ok(out),
        Err(err) => return Err(err.into()),
    };
    let mut rows = stmt.query(params![workspace, anchor_id])?;
    while let Some(row) = rows.next()? {
        out.push(AnchorBindingRow {
            kind: row.get(0)?,
            repo_rel: row.get(1)?,
            created_at_ms: row.get(2)?,
            updated_at_ms: row.get(3)?,
        });
    }
    Ok(out)
}

impl SqliteStore {
    pub fn anchor_bindings_list_for_anchor(
        &mut self,
        workspace: &WorkspaceId,
        anchor_id: &str,
    ) -> Result<Vec<AnchorBindingRow>, StoreError> {
        let anchor_id = crate::store::anchors::normalize_anchor_id(anchor_id)?;
        let tx = self.conn.transaction()?;
        let bindings = anchor_bindings_list_for_anchor_tx(&tx, workspace.as_str(), &anchor_id)?;
        tx.commit()?;
        Ok(bindings)
    }

    pub fn anchor_bindings_lookup_any(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorBindingsLookupAnyRequest,
    ) -> Result<AnchorBindingsLookupAnyResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_LOOKUP_LIMIT);
        if limit == 0 {
            return Ok(AnchorBindingsLookupAnyResult {
                bindings: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;

        let mut repo_rels = request
            .repo_rels
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        repo_rels.sort();
        repo_rels.dedup();
        if repo_rels.is_empty() {
            return Ok(AnchorBindingsLookupAnyResult {
                bindings: Vec::new(),
                has_more: false,
            });
        }

        let tx = self.conn.transaction()?;

        let mut placeholders = String::new();
        for idx in 0..repo_rels.len() {
            if idx > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }

        let sql = format!(
            r#"
            SELECT anchor_id, kind, repo_rel, created_at_ms, updated_at_ms
            FROM anchor_bindings
            WHERE workspace=? AND repo_rel IN ({placeholders})
            ORDER BY LENGTH(repo_rel) DESC, updated_at_ms DESC, anchor_id ASC, repo_rel ASC
            LIMIT ?
            "#,
            placeholders = placeholders
        );

        let mut params = Vec::<rusqlite::types::Value>::new();
        params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
        for repo_rel in &repo_rels {
            params.push(rusqlite::types::Value::Text(repo_rel.to_string()));
        }
        params.push(rusqlite::types::Value::Integer(query_limit));

        let mut out = Vec::<AnchorBindingHit>::new();
        let mut stmt = match tx.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(err) if is_missing_table(&err, "anchor_bindings") => {
                return Ok(AnchorBindingsLookupAnyResult {
                    bindings: Vec::new(),
                    has_more: false,
                });
            }
            Err(err) => return Err(err.into()),
        };
        let mut rows = stmt.query(params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            out.push(AnchorBindingHit {
                anchor_id: row.get(0)?,
                kind: row.get(1)?,
                repo_rel: row.get(2)?,
                created_at_ms: row.get(3)?,
                updated_at_ms: row.get(4)?,
            });
        }
        drop(rows);
        drop(stmt);
        tx.commit()?;

        let has_more = out.len() > limit;
        out.truncate(limit);
        Ok(AnchorBindingsLookupAnyResult {
            bindings: out,
            has_more,
        })
    }

    pub fn anchor_bindings_list_for_anchors_any(
        &mut self,
        workspace: &WorkspaceId,
        anchor_ids: Vec<String>,
    ) -> Result<Vec<AnchorBindingHit>, StoreError> {
        let mut anchor_ids = anchor_ids
            .into_iter()
            .map(|raw| crate::store::anchors::normalize_anchor_id(&raw))
            .collect::<Result<Vec<_>, _>>()?;
        anchor_ids.sort();
        anchor_ids.dedup();
        if anchor_ids.is_empty() {
            return Ok(Vec::new());
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
            SELECT anchor_id, kind, repo_rel, created_at_ms, updated_at_ms
            FROM anchor_bindings
            WHERE workspace=? AND anchor_id IN ({placeholders})
            ORDER BY anchor_id ASC, kind ASC, repo_rel ASC
            "#,
            placeholders = placeholders
        );

        let mut params = Vec::<rusqlite::types::Value>::new();
        params.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
        for id in &anchor_ids {
            params.push(rusqlite::types::Value::Text(id.to_string()));
        }

        let mut out = Vec::<AnchorBindingHit>::new();
        let mut stmt = match tx.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(err) if is_missing_table(&err, "anchor_bindings") => {
                return Ok(out);
            }
            Err(err) => return Err(err.into()),
        };
        let mut rows = stmt.query(params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            out.push(AnchorBindingHit {
                anchor_id: row.get(0)?,
                kind: row.get(1)?,
                repo_rel: row.get(2)?,
                created_at_ms: row.get(3)?,
                updated_at_ms: row.get(4)?,
            });
        }
        drop(rows);
        drop(stmt);
        tx.commit()?;
        Ok(out)
    }

    pub fn anchor_bindings_index_list(
        &mut self,
        workspace: &WorkspaceId,
        mut request: AnchorBindingsIndexListRequest,
    ) -> Result<AnchorBindingsIndexListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_INDEX_LIST_LIMIT);
        if limit == 0 {
            return Ok(AnchorBindingsIndexListResult {
                bindings: Vec::new(),
                has_more: false,
            });
        }
        let offset = request.offset;
        let query_limit = limit.saturating_add(1) as i64;

        if let Some(anchor_id) = request.anchor_id.as_ref() {
            request.anchor_id = Some(crate::store::anchors::normalize_anchor_id(anchor_id)?);
        }

        let tx = self.conn.transaction()?;

        let mut out = Vec::<AnchorBindingIndexRow>::new();
        let mut stmt = match tx.prepare(
            r#"
            SELECT b.anchor_id, a.title, a.kind, b.kind, b.repo_rel, b.created_at_ms, b.updated_at_ms
            FROM anchor_bindings b
            LEFT JOIN anchors a
              ON a.workspace=b.workspace AND a.id=b.anchor_id
            WHERE b.workspace=?1
              AND b.kind='path'
              AND (?2 IS NULL OR b.anchor_id=?2)
              AND (
                ?3 IS NULL
                OR b.repo_rel=?3
                OR b.repo_rel LIKE (?3 || '/%')
              )
            ORDER BY b.repo_rel ASC, b.anchor_id ASC, b.kind ASC
            LIMIT ?4 OFFSET ?5
            "#,
        ) {
            Ok(stmt) => stmt,
            Err(err) if is_missing_table(&err, "anchor_bindings") => {
                return Ok(AnchorBindingsIndexListResult {
                    bindings: Vec::new(),
                    has_more: false,
                });
            }
            Err(err) => return Err(err.into()),
        };

        let mut rows = stmt.query(params![
            workspace.as_str(),
            request.anchor_id.as_deref(),
            request.prefix.as_deref(),
            query_limit,
            offset as i64
        ])?;
        while let Some(row) = rows.next()? {
            out.push(AnchorBindingIndexRow {
                anchor_id: row.get(0)?,
                anchor_title: row.get(1)?,
                anchor_kind: row.get(2)?,
                kind: row.get(3)?,
                repo_rel: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            });
        }
        drop(rows);
        drop(stmt);
        tx.commit()?;

        let has_more = out.len() > limit;
        out.truncate(limit);
        Ok(AnchorBindingsIndexListResult {
            bindings: out,
            has_more,
        })
    }
}
