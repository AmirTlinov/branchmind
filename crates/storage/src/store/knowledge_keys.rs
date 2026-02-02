#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};

const KNOWLEDGE_KEY_TAG_PREFIX: &str = "k:";
const MAX_KEY_SLUG_LEN: usize = 64;
const MAX_LIST_LIMIT: usize = 200;

fn normalize_key_slug(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.len() > MAX_KEY_SLUG_LEN {
        return None;
    }
    let mut chars = raw.chars();
    let first = chars.next()?;
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return None;
    }
    for ch in chars {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            continue;
        }
        return None;
    }
    Some(raw.to_string())
}

fn extract_key_slug(tags: &[String]) -> Option<String> {
    let mut out: Option<String> = None;
    for tag in tags {
        let tag = tag.trim();
        let Some(rest) = tag.strip_prefix(KNOWLEDGE_KEY_TAG_PREFIX) else {
            continue;
        };
        let slug = normalize_key_slug(rest)?;
        match &out {
            None => out = Some(slug),
            Some(existing) if existing == &slug => {}
            Some(_) => return None, // multiple distinct keys -> ignore (best-effort)
        }
    }
    out
}

fn normalize_anchor_id_from_tag(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = if raw.to_ascii_lowercase().starts_with("a:") {
        raw.to_string()
    } else {
        format!("a:{raw}")
    };
    crate::store::anchors::normalize_anchor_id(&candidate).ok()
}

fn extract_anchor_ids(tags: &[String]) -> Vec<String> {
    let mut out = Vec::<String>::new();
    for tag in tags {
        let Some(id) = normalize_anchor_id_from_tag(tag) else {
            continue;
        };
        out.push(id);
    }
    out.sort();
    out.dedup();
    out
}

pub(in crate::store) struct UpsertKnowledgeKeysForCardTxArgs<'a> {
    pub(in crate::store) card_id: &'a str,
    pub(in crate::store) card_type: &'a str,
    pub(in crate::store) tags: &'a [String],
    pub(in crate::store) now_ms: i64,
}

pub(in crate::store) fn upsert_knowledge_keys_for_card_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    args: UpsertKnowledgeKeysForCardTxArgs<'_>,
) -> Result<(), StoreError> {
    let UpsertKnowledgeKeysForCardTxArgs {
        card_id,
        card_type,
        tags,
        now_ms,
    } = args;

    if !card_type.trim().eq_ignore_ascii_case("knowledge") {
        return Ok(());
    }

    let Some(key) = extract_key_slug(tags) else {
        return Ok(());
    };
    let anchor_ids = extract_anchor_ids(tags);
    if anchor_ids.is_empty() {
        return Ok(());
    }

    for anchor_id in anchor_ids {
        let existing = tx
            .query_row(
                r#"
                SELECT card_id
                FROM knowledge_keys
                WHERE workspace=?1 AND anchor_id=?2 AND key=?3
                LIMIT 1
                "#,
                params![workspace, anchor_id.as_str(), key.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing == card_id {
                tx.execute(
                    r#"
                    UPDATE knowledge_keys
                    SET updated_at_ms=?4
                    WHERE workspace=?1 AND anchor_id=?2 AND key=?3
                    "#,
                    params![workspace, anchor_id.as_str(), key.as_str(), now_ms],
                )?;
            } else {
                // Evolvable knowledge: (anchor,key) points to the *latest* card_id.
                // The graph keeps history; the index is the fast "current" pointer.
                tx.execute(
                    r#"
                    UPDATE knowledge_keys
                    SET card_id=?4, updated_at_ms=?5
                    WHERE workspace=?1 AND anchor_id=?2 AND key=?3
                    "#,
                    params![workspace, anchor_id.as_str(), key.as_str(), card_id, now_ms],
                )?;
            }
        } else {
            tx.execute(
                r#"
                INSERT INTO knowledge_keys(workspace, anchor_id, key, card_id, created_at_ms, updated_at_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    workspace,
                    anchor_id.as_str(),
                    key.as_str(),
                    card_id,
                    now_ms,
                    now_ms
                ],
            )?;
        }
    }

    Ok(())
}

impl SqliteStore {
    pub fn knowledge_keys_list_any(
        &mut self,
        workspace: &WorkspaceId,
        request: KnowledgeKeysListAnyRequest,
    ) -> Result<KnowledgeKeysListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_LIST_LIMIT);
        if limit == 0 {
            return Ok(KnowledgeKeysListResult {
                items: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;

        let mut anchor_ids = Vec::<String>::new();
        for raw in request.anchor_ids.into_iter() {
            if let Some(id) = normalize_anchor_id_from_tag(&raw) {
                anchor_ids.push(id);
            }
        }
        anchor_ids.sort();
        anchor_ids.dedup();

        let tx = self.conn.transaction()?;

        let mut items = Vec::<KnowledgeKeyRow>::new();
        if anchor_ids.is_empty() {
            let mut stmt = tx.prepare(
                r#"
                SELECT anchor_id, key, card_id, created_at_ms, updated_at_ms
                FROM knowledge_keys
                WHERE workspace=?1
                ORDER BY updated_at_ms DESC, anchor_id ASC, key ASC
                LIMIT ?2
                "#,
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), query_limit])?;
            while let Some(row) = rows.next()? {
                items.push(KnowledgeKeyRow {
                    anchor_id: row.get(0)?,
                    key: row.get(1)?,
                    card_id: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                });
            }
        } else {
            let mut placeholders = String::new();
            for idx in 0..anchor_ids.len() {
                if idx > 0 {
                    placeholders.push(',');
                }
                placeholders.push('?');
            }
            let sql = format!(
                r#"
                SELECT anchor_id, key, card_id, created_at_ms, updated_at_ms
                FROM knowledge_keys
                WHERE workspace=? AND anchor_id IN ({placeholders})
                ORDER BY updated_at_ms DESC, anchor_id ASC, key ASC
                LIMIT ?
                "#,
                placeholders = placeholders
            );

            let mut params_vec = Vec::<rusqlite::types::Value>::new();
            params_vec.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
            for id in &anchor_ids {
                params_vec.push(rusqlite::types::Value::Text(id.clone()));
            }
            params_vec.push(rusqlite::types::Value::Integer(query_limit));

            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params_vec))?;
            while let Some(row) = rows.next()? {
                items.push(KnowledgeKeyRow {
                    anchor_id: row.get(0)?,
                    key: row.get(1)?,
                    card_id: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                });
            }
        }

        let has_more = items.len() > limit;
        items.truncate(limit);

        tx.commit()?;
        Ok(KnowledgeKeysListResult { items, has_more })
    }

    pub fn knowledge_keys_list_by_key(
        &mut self,
        workspace: &WorkspaceId,
        request: KnowledgeKeysListByKeyRequest,
    ) -> Result<KnowledgeKeysListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_LIST_LIMIT);
        if limit == 0 {
            return Ok(KnowledgeKeysListResult {
                items: Vec::new(),
                has_more: false,
            });
        }
        let query_limit = limit.saturating_add(1) as i64;

        let key_raw = request.key.trim();
        let key_raw = key_raw
            .strip_prefix(KNOWLEDGE_KEY_TAG_PREFIX)
            .unwrap_or(key_raw);
        let key = normalize_key_slug(key_raw).ok_or_else(|| {
            StoreError::InvalidInput(
                "key must be a valid slug (<slug> or k:<slug>; lowercase letters/digits + '-')",
            )
        })?;

        let mut anchor_ids = Vec::<String>::new();
        for raw in request.anchor_ids.into_iter() {
            if let Some(id) = normalize_anchor_id_from_tag(&raw) {
                anchor_ids.push(id);
            }
        }
        anchor_ids.sort();
        anchor_ids.dedup();

        let tx = self.conn.transaction()?;

        let mut items = Vec::<KnowledgeKeyRow>::new();
        if anchor_ids.is_empty() {
            let mut stmt = tx.prepare(
                r#"
                SELECT anchor_id, key, card_id, created_at_ms, updated_at_ms
                FROM knowledge_keys
                WHERE workspace=?1 AND key=?2
                ORDER BY updated_at_ms DESC, anchor_id ASC
                LIMIT ?3
                "#,
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), key.as_str(), query_limit])?;
            while let Some(row) = rows.next()? {
                items.push(KnowledgeKeyRow {
                    anchor_id: row.get(0)?,
                    key: row.get(1)?,
                    card_id: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                });
            }
        } else {
            let mut placeholders = String::new();
            for idx in 0..anchor_ids.len() {
                if idx > 0 {
                    placeholders.push(',');
                }
                placeholders.push('?');
            }
            let sql = format!(
                r#"
                SELECT anchor_id, key, card_id, created_at_ms, updated_at_ms
                FROM knowledge_keys
                WHERE workspace=? AND key=? AND anchor_id IN ({placeholders})
                ORDER BY updated_at_ms DESC, anchor_id ASC
                LIMIT ?
                "#,
                placeholders = placeholders
            );

            let mut params_vec = Vec::<rusqlite::types::Value>::new();
            params_vec.push(rusqlite::types::Value::Text(workspace.as_str().to_string()));
            params_vec.push(rusqlite::types::Value::Text(key.clone()));
            for id in &anchor_ids {
                params_vec.push(rusqlite::types::Value::Text(id.clone()));
            }
            params_vec.push(rusqlite::types::Value::Integer(query_limit));

            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params_vec))?;
            while let Some(row) = rows.next()? {
                items.push(KnowledgeKeyRow {
                    anchor_id: row.get(0)?,
                    key: row.get(1)?,
                    card_id: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                });
            }
        }

        let has_more = items.len() > limit;
        items.truncate(limit);

        tx.commit()?;
        Ok(KnowledgeKeysListResult { items, has_more })
    }
}
