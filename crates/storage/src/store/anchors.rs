#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, Transaction, params};

const MAP_ANCHOR_ID_PREFIX: &str = "a:";
const MAX_ANCHOR_SLUG_LEN: usize = 64;
const MAX_ANCHOR_TITLE_LEN: usize = 120;
const MAX_ANCHOR_DESC_LEN: usize = 280;
const MAX_ANCHOR_REFS: usize = 32;
const MAX_ANCHOR_ALIASES: usize = 32;
const MAX_ANCHOR_DEPENDS: usize = 32;
const MAX_LIST_LIMIT: usize = 200;

pub(super) fn normalize_anchor_id(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("anchor.id must not be empty"));
    }
    let lowered = raw.to_ascii_lowercase();
    let Some(slug) = lowered.strip_prefix(MAP_ANCHOR_ID_PREFIX) else {
        return Err(StoreError::InvalidInput("anchor.id must start with a:"));
    };
    if slug.is_empty() {
        return Err(StoreError::InvalidInput("anchor.id slug must not be empty"));
    }
    if slug.len() > MAX_ANCHOR_SLUG_LEN {
        return Err(StoreError::InvalidInput("anchor.id slug is too long"));
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return Err(StoreError::InvalidInput("anchor.id slug must not be empty"));
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(StoreError::InvalidInput(
            "anchor.id slug must start with [a-z0-9]",
        ));
    }
    for ch in chars {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            continue;
        }
        return Err(StoreError::InvalidInput(
            "anchor.id slug contains invalid characters",
        ));
    }
    Ok(format!("{MAP_ANCHOR_ID_PREFIX}{slug}"))
}

fn title_from_anchor_id(anchor_id: &str) -> String {
    let raw = anchor_id.trim();
    let slug = raw.strip_prefix(MAP_ANCHOR_ID_PREFIX).unwrap_or(raw).trim();

    let words = slug
        .split('-')
        .filter(|w| !w.trim().is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();

    let title = if words.is_empty() {
        "Anchor".to_string()
    } else {
        words.join(" ")
    };

    // Stable truncation (chars) so we stay within DB constraints deterministically.
    title.chars().take(MAX_ANCHOR_TITLE_LEN).collect()
}

pub(in crate::store) fn ensure_anchor_exists_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    raw_id: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    let id = normalize_anchor_id(raw_id)?;

    let exists = tx
        .query_row(
            "SELECT 1 FROM anchors WHERE workspace=?1 AND id=?2 LIMIT 1",
            params![workspace, id.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        return Ok(());
    }

    // Do not create anchors that collide with alias ids; aliases remain redirects.
    let is_alias = tx
        .query_row(
            "SELECT 1 FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
            params![workspace, id.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if is_alias {
        return Ok(());
    }

    let title = title_from_anchor_id(&id);
    let kind = "component".to_string();
    let status = "active".to_string();

    let empty: Vec<String> = Vec::new();
    let refs_json = encode_json_string_list(&empty);
    let depends_on_json = encode_json_string_list(&empty);

    let _ = tx.execute(
        r#"
        INSERT OR IGNORE INTO anchors(
          workspace, id, title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            workspace,
            id.as_str(),
            title,
            kind,
            Option::<String>::None,
            refs_json,
            Option::<String>::None,
            depends_on_json,
            status,
            now_ms,
            now_ms
        ],
    )?;

    Ok(())
}

fn normalize_anchor_kind(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("anchor.kind must not be empty"));
    }
    let k = raw.to_ascii_lowercase();
    let ok = matches!(
        k.as_str(),
        "boundary" | "component" | "contract" | "data" | "test-surface" | "ops"
    );
    if !ok {
        return Err(StoreError::InvalidInput("anchor.kind is invalid"));
    }
    Ok(k)
}

fn normalize_anchor_status(raw: &str) -> Result<String, StoreError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StoreError::InvalidInput("anchor.status must not be empty"));
    }
    let s = raw.to_ascii_lowercase();
    if !matches!(s.as_str(), "active" | "deprecated") {
        return Err(StoreError::InvalidInput("anchor.status is invalid"));
    }
    Ok(s)
}

fn normalize_string_list(
    items: Vec<String>,
    max_items: usize,
    max_item_len: usize,
) -> Result<Vec<String>, StoreError> {
    if items.len() > max_items {
        return Err(StoreError::InvalidInput("list exceeds max_items"));
    }
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > max_item_len {
            return Err(StoreError::InvalidInput("list item is too long"));
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

pub(in crate::store) fn encode_json_string_list(items: &[String]) -> String {
    // Deterministic encoding (stable ordering already ensured by callers).
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

pub(in crate::store) fn decode_json_string_list(
    raw: Option<String>,
) -> Result<Vec<String>, StoreError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(trimmed)
        .map_err(|_| StoreError::InvalidInput("stored anchor list is invalid json"))
}

impl SqliteStore {
    pub fn count_anchors(&self, workspace: &WorkspaceId) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM anchors WHERE workspace=?1",
            params![workspace.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn anchor_get(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorGetRequest,
    ) -> Result<Option<AnchorRow>, StoreError> {
        let id = normalize_anchor_id(&request.id)?;
        let tx = self.conn.transaction()?;
        let row: Option<(AnchorRow, Option<String>, Option<String>)> = tx
            .query_row(
                r#"
                SELECT title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms
                FROM anchors
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id.as_str()],
                |row| {
                    let anchor = AnchorRow {
                        id: id.clone(),
                        title: row.get(0)?,
                        kind: row.get(1)?,
                        description: row.get(2)?,
                        refs: Vec::new(),
                        aliases: Vec::new(),
                        parent_id: row.get(4)?,
                        depends_on: Vec::new(),
                        status: row.get(6)?,
                        created_at_ms: row.get(7)?,
                        updated_at_ms: row.get(8)?,
                    };
                    let refs_json: Option<String> = row.get(3)?;
                    let depends_on_json: Option<String> = row.get(5)?;
                    Ok((anchor, refs_json, depends_on_json))
                },
            )
            .optional()?;
        let Some((mut anchor, refs_json, depends_on_json)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        anchor.refs = decode_json_string_list(refs_json)?;
        anchor.refs.sort();
        anchor.refs.dedup();
        anchor.depends_on = decode_json_string_list(depends_on_json)?;
        anchor.depends_on.sort();
        anchor.depends_on.dedup();
        anchor.aliases = crate::store::anchor_aliases::anchor_aliases_list_for_anchor_tx(
            &tx,
            workspace.as_str(),
            id.as_str(),
        )?;
        anchor.aliases.sort();
        anchor.aliases.dedup();
        tx.commit()?;
        Ok(Some(anchor))
    }

    pub fn anchors_list(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorsListRequest,
    ) -> Result<AnchorsListResult, StoreError> {
        let limit = request.limit.clamp(0, MAX_LIST_LIMIT);
        let query_limit = limit.saturating_add(1) as i64;

        let text = request.text.map(|s| s.trim().to_ascii_lowercase());
        let text = text.filter(|s| !s.is_empty());
        let text_like = text.as_ref().map(|s| format!("%{s}%"));

        let kind = request.kind.map(|s| s.trim().to_ascii_lowercase());
        let kind = kind.filter(|s| !s.is_empty());

        let status = request.status.map(|s| s.trim().to_ascii_lowercase());
        let status = status.filter(|s| !s.is_empty());

        let tx = self.conn.transaction()?;

        let mut stmt = tx.prepare(
            r#"
            SELECT id, title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms
            FROM anchors
            WHERE workspace=?1
              AND (?2 IS NULL OR LOWER(id) LIKE ?2 OR LOWER(title) LIKE ?2)
              AND (?3 IS NULL OR kind=?3)
              AND (?4 IS NULL OR status=?4)
            ORDER BY id ASC
            LIMIT ?5
            "#,
        )?;

        let mut rows = stmt.query(params![
            workspace.as_str(),
            text_like.as_deref(),
            kind.as_deref(),
            status.as_deref(),
            query_limit,
        ])?;

        let mut anchors = Vec::<AnchorRow>::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let refs_json: Option<String> = row.get(4)?;
            let depends_on_json: Option<String> = row.get(6)?;

            let mut anchor = AnchorRow {
                id,
                title: row.get(1)?,
                kind: row.get(2)?,
                description: row.get(3)?,
                refs: decode_json_string_list(refs_json)?,
                aliases: Vec::new(),
                parent_id: row.get(5)?,
                depends_on: decode_json_string_list(depends_on_json)?,
                status: row.get(7)?,
                created_at_ms: row.get(8)?,
                updated_at_ms: row.get(9)?,
            };
            // Normalize loaded data to keep outputs deterministic even when old rows were messy.
            anchor.refs.sort();
            anchor.refs.dedup();
            anchor.depends_on.sort();
            anchor.depends_on.dedup();

            anchors.push(anchor);
        }
        drop(rows);
        drop(stmt);

        let has_more = anchors.len() > limit;
        anchors.truncate(limit);

        for anchor in anchors.iter_mut() {
            anchor.aliases = crate::store::anchor_aliases::anchor_aliases_list_for_anchor_tx(
                &tx,
                workspace.as_str(),
                anchor.id.as_str(),
            )?;
            anchor.aliases.sort();
            anchor.aliases.dedup();
        }

        tx.commit()?;
        Ok(AnchorsListResult { anchors, has_more })
    }

    pub fn anchor_upsert(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorUpsertRequest,
    ) -> Result<AnchorUpsertResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        let out = anchor_upsert_tx(&tx, workspace, request, now_ms)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn anchor_rename(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorRenameRequest,
    ) -> Result<AnchorRenameResult, StoreError> {
        let from_id = normalize_anchor_id(&request.from_id)?;
        let to_id = normalize_anchor_id(&request.to_id)?;
        if from_id == to_id {
            return Err(StoreError::InvalidInput(
                "anchor.to_id must not equal anchor.from_id",
            ));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        type AnchorRenameSourceRow = (
            String,         // title
            String,         // kind
            Option<String>, // description
            Option<String>, // refs_json
            Option<String>, // parent_id
            Option<String>, // depends_on_json
            String,         // status
            i64,            // created_at_ms
            i64,            // updated_at_ms
        );

        let row: Option<AnchorRenameSourceRow> = tx
            .query_row(
                r#"
                SELECT title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms
                FROM anchors
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), from_id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            title,
            kind,
            description,
            refs_json,
            mut parent_id,
            depends_on_json,
            status,
            created_at_ms,
            _updated_at_ms,
        )) = row
        else {
            return Err(StoreError::InvalidInput("anchor.from_id not found"));
        };

        let to_exists = tx
            .query_row(
                "SELECT 1 FROM anchors WHERE workspace=?1 AND id=?2 LIMIT 1",
                params![workspace.as_str(), to_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if to_exists {
            return Err(StoreError::InvalidInput("anchor.to_id already exists"));
        }

        let to_is_alias = tx
            .query_row(
                "SELECT 1 FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
                params![workspace.as_str(), to_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if to_is_alias {
            return Err(StoreError::InvalidInput(
                "anchor.to_id collides with an existing alias",
            ));
        }

        // Load existing alias ids (they will remain valid aliases after rename).
        let mut aliases = crate::store::anchor_aliases::anchor_aliases_list_for_anchor_tx(
            &tx,
            workspace.as_str(),
            from_id.as_str(),
        )?;
        aliases.sort();
        aliases.dedup();
        if aliases.iter().any(|a| a == to_id.as_str()) {
            return Err(StoreError::InvalidInput(
                "anchor.to_id must not match an existing alias id",
            ));
        }

        // When renaming, we keep historical aliases for discovery, but relations should stay
        // canonical. We therefore rewrite references to both the old id and any existing aliases.
        let mut rewrite_ids = std::collections::BTreeSet::<String>::new();
        rewrite_ids.insert(from_id.clone());
        for a in &aliases {
            rewrite_ids.insert(a.clone());
        }

        let mut refs = decode_json_string_list(refs_json)?;
        refs.sort();
        refs.dedup();

        if parent_id.as_deref() == Some(from_id.as_str())
            || parent_id.as_deref() == Some(to_id.as_str())
            || parent_id
                .as_deref()
                .is_some_and(|p| rewrite_ids.contains(p))
        {
            parent_id = None;
        }

        let mut depends_on = decode_json_string_list(depends_on_json)?;
        depends_on.retain(|d| d != &from_id && d != &to_id && !rewrite_ids.contains(d));
        depends_on.sort();
        depends_on.dedup();

        let refs_json = encode_json_string_list(&refs);
        let depends_json = encode_json_string_list(&depends_on);

        // Insert the new anchor row (preserving created_at_ms).
        tx.execute(
            r#"
            INSERT INTO anchors(workspace, id, title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                workspace.as_str(),
                to_id.as_str(),
                title.as_str(),
                kind.as_str(),
                description.as_deref(),
                refs_json,
                parent_id.as_deref(),
                depends_json,
                status.as_str(),
                created_at_ms,
                now_ms,
            ],
        )?;

        // Update references in other anchors deterministically.
        // parent_id is a plain string column.
        for old in rewrite_ids.iter() {
            tx.execute(
                "UPDATE anchors SET parent_id=?3 WHERE workspace=?1 AND parent_id=?2",
                params![workspace.as_str(), old.as_str(), to_id.as_str()],
            )?;
        }

        // depends_on_json is stored as a json array; update by scanning anchors in the workspace.
        let mut stmt = tx.prepare(
            r#"
            SELECT id, depends_on_json
            FROM anchors
            WHERE workspace=?1
            ORDER BY id ASC
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str()])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            if id == from_id {
                continue;
            }
            let depends_raw: Option<String> = row.get(1)?;
            let mut deps = decode_json_string_list(depends_raw)?;
            let mut changed = false;
            for dep in deps.iter_mut() {
                if rewrite_ids.contains(dep) {
                    *dep = to_id.clone();
                    changed = true;
                }
            }
            if changed {
                deps.sort();
                deps.dedup();
                let new_json = encode_json_string_list(&deps);
                tx.execute(
                    "UPDATE anchors SET depends_on_json=?3 WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id.as_str(), new_json],
                )?;
            }
        }
        drop(rows);
        drop(stmt);

        // Move alias rows to the new anchor id and add a "from → to" alias mapping.
        tx.execute(
            "DELETE FROM anchor_aliases WHERE workspace=?1 AND anchor_id=?2",
            params![workspace.as_str(), from_id.as_str()],
        )?;
        for alias in &aliases {
            tx.execute(
                "INSERT INTO anchor_aliases(workspace, alias_id, anchor_id) VALUES (?1, ?2, ?3)",
                params![workspace.as_str(), alias.as_str(), to_id.as_str()],
            )?;
        }
        tx.execute(
            "INSERT INTO anchor_aliases(workspace, alias_id, anchor_id) VALUES (?1, ?2, ?3)",
            params![workspace.as_str(), from_id.as_str(), to_id.as_str()],
        )?;

        // Remove old anchor record.
        tx.execute(
            "DELETE FROM anchors WHERE workspace=?1 AND id=?2",
            params![workspace.as_str(), from_id.as_str()],
        )?;

        tx.commit()?;

        let mut final_aliases = aliases;
        final_aliases.push(from_id.clone());
        final_aliases.sort();
        final_aliases.dedup();

        Ok(AnchorRenameResult {
            from_id: from_id.clone(),
            to_id: to_id.clone(),
            anchor: AnchorRow {
                id: to_id,
                title,
                kind,
                description,
                refs,
                aliases: final_aliases,
                parent_id,
                depends_on,
                status,
                created_at_ms,
                updated_at_ms: now_ms,
            },
        })
    }

    pub fn anchors_bootstrap(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorsBootstrapRequest,
    ) -> Result<AnchorsBootstrapResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        // Pre-normalize ids so sorting is deterministic and invalid ids fail before any writes.
        let mut items = Vec::<AnchorUpsertRequest>::new();
        for mut a in request.anchors {
            a.id = normalize_anchor_id(&a.id)?;
            items.push(a);
        }
        items.sort_by(|a, b| a.id.cmp(&b.id));
        for window in items.windows(2) {
            if window[0].id == window[1].id {
                return Err(StoreError::InvalidInput(
                    "anchors_bootstrap has duplicate ids",
                ));
            }
        }

        let mut results = Vec::<AnchorUpsertResult>::new();
        for item in items {
            results.push(anchor_upsert_tx(&tx, workspace, item, now_ms)?);
        }

        tx.commit()?;
        Ok(AnchorsBootstrapResult { anchors: results })
    }
}

fn anchor_upsert_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    request: AnchorUpsertRequest,
    now_ms: i64,
) -> Result<AnchorUpsertResult, StoreError> {
    let id = normalize_anchor_id(&request.id)?;

    let title = request.title.trim();
    if title.is_empty() {
        return Err(StoreError::InvalidInput("anchor.title must not be empty"));
    }
    if title.len() > MAX_ANCHOR_TITLE_LEN {
        return Err(StoreError::InvalidInput("anchor.title is too long"));
    }

    let kind = normalize_anchor_kind(&request.kind)?;

    let description = request
        .description
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(desc) = description.as_ref()
        && desc.len() > MAX_ANCHOR_DESC_LEN
    {
        return Err(StoreError::InvalidInput("anchor.description is too long"));
    }

    let mut refs = normalize_string_list(request.refs, MAX_ANCHOR_REFS, 220)?;
    refs.sort();

    let mut aliases = Vec::<String>::new();
    for raw in request.aliases.into_iter() {
        aliases.push(normalize_anchor_id(&raw)?);
    }
    aliases = normalize_string_list(aliases, MAX_ANCHOR_ALIASES, 66)?;
    if aliases.iter().any(|a| a == id.as_str()) {
        return Err(StoreError::InvalidInput(
            "anchor.aliases must not include anchor.id",
        ));
    }
    aliases.sort();

    let parent_id = match request.parent_id {
        Some(v) => Some(normalize_anchor_id(&v)?),
        None => None,
    };
    if parent_id.as_deref() == Some(id.as_str()) {
        return Err(StoreError::InvalidInput(
            "anchor.parent_id must not equal anchor.id",
        ));
    }

    let mut depends_on = Vec::<String>::new();
    for raw in request.depends_on.into_iter() {
        depends_on.push(normalize_anchor_id(&raw)?);
    }
    depends_on = normalize_string_list(depends_on, MAX_ANCHOR_DEPENDS, 66)?;
    if depends_on.iter().any(|d| d == id.as_str()) {
        return Err(StoreError::InvalidInput(
            "anchor.depends_on must not include anchor.id",
        ));
    }
    depends_on.sort();

    let status = normalize_anchor_status(&request.status)?;

    ensure_workspace_tx(tx, workspace, now_ms)?;

    // Validate alias uniqueness inside the workspace:
    // - aliases must not collide with existing anchor ids,
    // - aliases must not be owned by another anchor.
    for alias in &aliases {
        let collides_with_anchor_id = tx
            .query_row(
                "SELECT 1 FROM anchors WHERE workspace=?1 AND id=?2 LIMIT 1",
                params![workspace.as_str(), alias.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if collides_with_anchor_id {
            return Err(StoreError::InvalidInput(
                "anchor.aliases must not include an existing anchor id",
            ));
        }

        let owner: Option<String> = tx
            .query_row(
                "SELECT anchor_id FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
                params![workspace.as_str(), alias.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(owner) = owner
            && owner != id
        {
            return Err(StoreError::InvalidInput(
                "anchor.aliases contains an alias owned by another anchor",
            ));
        }
    }

    let existing_created_at_ms: Option<i64> = tx
        .query_row(
            "SELECT created_at_ms FROM anchors WHERE workspace=?1 AND id=?2",
            params![workspace.as_str(), id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    let existed = existing_created_at_ms.is_some();
    let created_at_ms = existing_created_at_ms.unwrap_or(now_ms);

    let refs_json = encode_json_string_list(&refs);
    let depends_json = encode_json_string_list(&depends_on);

    tx.execute(
        r#"
        INSERT INTO anchors(workspace, id, title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(workspace, id) DO UPDATE SET
          title=excluded.title,
          kind=excluded.kind,
          description=excluded.description,
          refs_json=excluded.refs_json,
          parent_id=excluded.parent_id,
          depends_on_json=excluded.depends_on_json,
          status=excluded.status,
          updated_at_ms=excluded.updated_at_ms
        "#,
        params![
            workspace.as_str(),
            id.as_str(),
            title,
            kind,
            description.as_deref(),
            refs_json,
            parent_id.as_deref(),
            depends_json,
            status,
            created_at_ms,
            now_ms,
        ],
    )?;

    // Replace alias rows atomically with the anchor upsert.
    tx.execute(
        "DELETE FROM anchor_aliases WHERE workspace=?1 AND anchor_id=?2",
        params![workspace.as_str(), id.as_str()],
    )?;
    for alias in &aliases {
        tx.execute(
            "INSERT INTO anchor_aliases(workspace, alias_id, anchor_id) VALUES (?1, ?2, ?3)",
            params![workspace.as_str(), alias.as_str(), id.as_str()],
        )?;
    }

    Ok(AnchorUpsertResult {
        anchor: AnchorRow {
            id,
            title: title.to_string(),
            kind,
            description,
            refs,
            aliases,
            parent_id,
            depends_on,
            status,
            created_at_ms,
            updated_at_ms: now_ms,
        },
        created: !existed,
    })
}
