#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

fn anchor_exists_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace: &str,
    id: &str,
) -> Result<bool, StoreError> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM anchors WHERE workspace=?1 AND id=?2 LIMIT 1",
            params![workspace, id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn alias_owner_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace: &str,
    alias_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT anchor_id FROM anchor_aliases WHERE workspace=?1 AND alias_id=?2 LIMIT 1",
            params![workspace, alias_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn replace_anchor_refs_in_depends_json(
    raw: Option<String>,
    replacements: &std::collections::BTreeSet<String>,
    into_id: &str,
    self_id: &str,
) -> Result<(Option<String>, bool), StoreError> {
    let mut deps = crate::store::anchors::decode_json_string_list(raw)?;
    let before = deps.clone();
    for dep in deps.iter_mut() {
        if replacements.contains(dep) {
            *dep = into_id.to_string();
        }
    }
    deps.retain(|d| d != self_id);
    deps.sort();
    deps.dedup();
    let changed = deps != before;
    let json = crate::store::anchors::encode_json_string_list(&deps);
    Ok((Some(json), changed))
}

impl SqliteStore {
    pub fn anchors_merge(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorsMergeRequest,
    ) -> Result<AnchorsMergeResult, StoreError> {
        let into_id = crate::store::anchors::normalize_anchor_id(&request.into_id)?;
        if request.from_ids.is_empty() {
            return Err(StoreError::InvalidInput(
                "anchors_merge.from must not be empty",
            ));
        }
        let mut from_ids = Vec::<String>::new();
        for raw in request.from_ids {
            from_ids.push(crate::store::anchors::normalize_anchor_id(&raw)?);
        }
        from_ids.sort();
        from_ids.dedup();
        if from_ids.iter().any(|id| id == &into_id) {
            return Err(StoreError::InvalidInput(
                "anchors_merge.from must not include into",
            ));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !anchor_exists_tx(&tx, workspace.as_str(), into_id.as_str())? {
            return Err(StoreError::InvalidInput("anchors_merge.into not found"));
        }

        // We rewrite relations not only for merged anchor ids, but also for any aliases owned by
        // those anchors, so the map stays canonical over time.
        let mut rewrite_ids = std::collections::BTreeSet::<String>::new();
        for id in &from_ids {
            rewrite_ids.insert(id.clone());
        }

        let mut merged_ids = Vec::<String>::new();
        let mut skipped_ids = Vec::<String>::new();

        // Pre-flight resolve: either the id exists as anchor, or it is already an alias to `into`.
        for id in &from_ids {
            if anchor_exists_tx(&tx, workspace.as_str(), id.as_str())? {
                continue;
            }
            let owner = alias_owner_tx(&tx, workspace.as_str(), id.as_str())?;
            match owner {
                Some(owner) if owner == into_id => {
                    skipped_ids.push(id.clone());
                }
                Some(_) => {
                    return Err(StoreError::InvalidInput(
                        "anchors_merge.from resolves to a different canonical anchor",
                    ));
                }
                None => {
                    return Err(StoreError::InvalidInput("anchors_merge.from not found"));
                }
            }
        }

        // Perform merge for anchors that exist as records.
        for id in &from_ids {
            if skipped_ids.iter().any(|s| s == id) {
                continue;
            }

            // Collect aliases owned by the anchor being merged to canonize relation fields.
            let mut aliases = crate::store::anchor_aliases::anchor_aliases_list_for_anchor_tx(
                &tx,
                workspace.as_str(),
                id.as_str(),
            )?;
            aliases.sort();
            aliases.dedup();
            for alias in &aliases {
                rewrite_ids.insert(alias.clone());
            }

            // Move alias rows to the canonical anchor id.
            tx.execute(
                "UPDATE anchor_aliases SET anchor_id=?3 WHERE workspace=?1 AND anchor_id=?2",
                params![workspace.as_str(), id.as_str(), into_id.as_str()],
            )?;

            // Add an explicit alias mapping `from_id → into_id` (unless already present).
            let owner = alias_owner_tx(&tx, workspace.as_str(), id.as_str())?;
            if let Some(owner) = owner {
                if owner != into_id {
                    return Err(StoreError::InvalidInput(
                        "anchors_merge.from collides with an existing alias owned by another anchor",
                    ));
                }
            } else {
                tx.execute(
                    "INSERT INTO anchor_aliases(workspace, alias_id, anchor_id) VALUES (?1, ?2, ?3)",
                    params![workspace.as_str(), id.as_str(), into_id.as_str()],
                )?;
            }

            // Move anchor path bindings (path→anchor map). Dedupe on collisions.
            match tx.execute(
                r#"
                INSERT OR IGNORE INTO anchor_bindings(workspace, anchor_id, kind, repo_rel, created_at_ms, updated_at_ms)
                SELECT workspace, ?3, kind, repo_rel, created_at_ms, ?4
                FROM anchor_bindings
                WHERE workspace=?1 AND anchor_id=?2
                "#,
                params![workspace.as_str(), id.as_str(), into_id.as_str(), now_ms],
            ) {
                Ok(_) => {}
                Err(err) if is_missing_table(&err, "anchor_bindings") => {}
                Err(err) => return Err(err.into()),
            }
            match tx.execute(
                "DELETE FROM anchor_bindings WHERE workspace=?1 AND anchor_id=?2",
                params![workspace.as_str(), id.as_str()],
            ) {
                Ok(_) => {}
                Err(err) if is_missing_table(&err, "anchor_bindings") => {}
                Err(err) => return Err(err.into()),
            }

            // Remove merged anchor record.
            tx.execute(
                "DELETE FROM anchors WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str()],
            )?;
            merged_ids.push(id.clone());
        }

        // Update relations in other anchors deterministically (including legacy alias ids).
        for old in rewrite_ids.iter() {
            tx.execute(
                "UPDATE anchors SET parent_id=?3 WHERE workspace=?1 AND parent_id=?2",
                params![workspace.as_str(), old.as_str(), into_id.as_str()],
            )?;
        }

        // Rewrite depends_on_json across anchors in the workspace.
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
            let depends_raw: Option<String> = row.get(1)?;

            let (new_json, changed) = replace_anchor_refs_in_depends_json(
                depends_raw,
                &rewrite_ids,
                into_id.as_str(),
                id.as_str(),
            )?;
            if !changed {
                continue;
            }
            tx.execute(
                "UPDATE anchors SET depends_on_json=?3 WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id.as_str(), new_json],
            )?;
        }
        drop(rows);
        drop(stmt);

        // Sanitize canonical anchor relations (avoid self-cycles after merge).
        let row: Option<(Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT parent_id, depends_on_json FROM anchors WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), into_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((parent_id, depends_on_json)) = row {
            let mut changed = false;

            let mut parent_id = parent_id;
            if let Some(p) = parent_id.as_deref()
                && (p == into_id.as_str() || rewrite_ids.contains(p))
            {
                parent_id = None;
                changed = true;
            }

            let mut deps = crate::store::anchors::decode_json_string_list(depends_on_json)?;
            let before = deps.clone();
            deps.retain(|d| d != &into_id && !rewrite_ids.contains(d));
            deps.sort();
            deps.dedup();
            if deps != before {
                changed = true;
            }

            if changed {
                let deps_json = crate::store::anchors::encode_json_string_list(&deps);
                tx.execute(
                    "UPDATE anchors SET parent_id=?3, depends_on_json=?4, updated_at_ms=?5 WHERE workspace=?1 AND id=?2",
                    params![
                        workspace.as_str(),
                        into_id.as_str(),
                        parent_id.as_deref(),
                        deps_json,
                        now_ms
                    ],
                )?;
            }
        }

        tx.commit()?;

        let Some(anchor) = self.anchor_get(
            workspace,
            AnchorGetRequest {
                id: into_id.clone(),
            },
        )?
        else {
            return Err(StoreError::InvalidInput(
                "anchors_merge.into not found after merge",
            ));
        };

        merged_ids.sort();
        merged_ids.dedup();
        skipped_ids.sort();
        skipped_ids.dedup();

        Ok(AnchorsMergeResult {
            into_id,
            from_ids,
            merged_ids,
            skipped_ids,
            anchor,
        })
    }
}
