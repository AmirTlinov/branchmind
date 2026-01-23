#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

const MAX_LINT_LIMIT: usize = 200;

fn severity_rank(severity: &str) -> i32 {
    match severity.trim().to_ascii_lowercase().as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn push_issue(
    issues: &mut Vec<AnchorsLintIssue>,
    code: &str,
    severity: &str,
    anchor: &str,
    message: String,
    hint: &str,
) {
    issues.push(AnchorsLintIssue {
        code: code.to_string(),
        severity: severity.to_string(),
        anchor: anchor.to_string(),
        message,
        hint: hint.to_string(),
    });
}

fn has_any_anchor_links_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace: &str,
    anchor_ids: &[String],
) -> Result<bool, StoreError> {
    if anchor_ids.is_empty() {
        return Ok(false);
    }
    let mut placeholders = String::new();
    for idx in 0..anchor_ids.len() {
        if idx > 0 {
            placeholders.push(',');
        }
        placeholders.push('?');
    }
    let sql = format!(
        "SELECT 1 FROM anchor_links WHERE workspace=? AND anchor_id IN ({}) LIMIT 1",
        placeholders
    );
    let mut params = Vec::<rusqlite::types::Value>::new();
    params.push(workspace.to_string().into());
    for id in anchor_ids {
        params.push(id.clone().into());
    }
    Ok(tx
        .query_row(&sql, rusqlite::params_from_iter(params), |_| Ok(()))
        .optional()?
        .is_some())
}

impl SqliteStore {
    pub fn anchors_lint(
        &mut self,
        workspace: &WorkspaceId,
        request: AnchorsLintRequest,
    ) -> Result<AnchorsLintResult, StoreError> {
        let limit = request.limit.clamp(1, MAX_LINT_LIMIT);

        let tx = self.conn.transaction()?;

        // Load anchors (canonical ids).
        let mut stmt = tx.prepare(
            r#"
            SELECT id, parent_id, depends_on_json, status
            FROM anchors
            WHERE workspace=?1
            ORDER BY id ASC
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str()])?;
        let mut anchors = Vec::<(String, Option<String>, Option<String>, String)>::new();
        while let Some(row) = rows.next()? {
            anchors.push((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?));
        }
        drop(rows);
        drop(stmt);

        let mut anchor_ids = std::collections::BTreeSet::<String>::new();
        for (id, _, _, _) in &anchors {
            anchor_ids.insert(id.clone());
        }

        // Load alias map.
        let mut alias_owner = std::collections::BTreeMap::<String, String>::new();
        let mut aliases_by_anchor = std::collections::BTreeMap::<String, Vec<String>>::new();
        let mut stmt = tx.prepare(
            r#"
            SELECT alias_id, anchor_id
            FROM anchor_aliases
            WHERE workspace=?1
            ORDER BY alias_id ASC
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str()])?;
        while let Some(row) = rows.next()? {
            let alias_id: String = row.get(0)?;
            let anchor_id: String = row.get(1)?;
            alias_owner.insert(alias_id.clone(), anchor_id.clone());
            aliases_by_anchor
                .entry(anchor_id)
                .or_default()
                .push(alias_id);
        }
        drop(rows);
        drop(stmt);

        for aliases in aliases_by_anchor.values_mut() {
            aliases.sort();
            aliases.dedup();
        }

        let mut issues = Vec::<AnchorsLintIssue>::new();

        // Alias integrity.
        for (alias_id, owner_id) in &alias_owner {
            if !anchor_ids.contains(owner_id) {
                push_issue(
                    &mut issues,
                    "ALIAS_DANGLING",
                    "error",
                    alias_id,
                    format!("alias {alias_id} points to missing anchor {owner_id}"),
                    "Fix: re-create the missing anchor or remove the dangling alias mapping.",
                );
            }
            if anchor_ids.contains(alias_id) {
                push_issue(
                    &mut issues,
                    "ALIAS_COLLIDES_WITH_ANCHOR_ID",
                    "warning",
                    alias_id,
                    format!("alias {alias_id} collides with an anchor id"),
                    "Fix: rename the alias or rename the anchor id to avoid ambiguity.",
                );
            }
        }

        // Relation integrity + orphan detection.
        let mut parent_map = std::collections::BTreeMap::<String, String>::new();
        for (id, parent_id, depends_on_json, status) in &anchors {
            if let Some(parent) = parent_id.as_deref() {
                parent_map.insert(id.clone(), parent.to_string());
            }

            if let Some(parent) = parent_id.as_deref() {
                if parent == id {
                    push_issue(
                        &mut issues,
                        "SELF_PARENT",
                        "error",
                        id,
                        "parent_id must not equal anchor id".to_string(),
                        "Fix: clear parent_id or point it at a different canonical anchor.",
                    );
                } else if anchor_ids.contains(parent) {
                    // ok (canonical)
                } else if let Some(owner) = alias_owner.get(parent) {
                    if owner == id {
                        push_issue(
                            &mut issues,
                            "SELF_PARENT",
                            "error",
                            id,
                            format!("parent_id {parent} resolves back to {id} via alias mapping"),
                            "Fix: clear parent_id or point it at a different canonical anchor.",
                        );
                    } else {
                        push_issue(
                            &mut issues,
                            "RELATION_USES_ALIAS",
                            "warning",
                            id,
                            format!("parent_id {parent} is an alias for {owner}"),
                            "Fix: rewrite relations to use canonical ids (anchors_rename/anchors_merge keep aliases for history only).",
                        );
                    }
                } else {
                    push_issue(
                        &mut issues,
                        "UNKNOWN_PARENT",
                        "error",
                        id,
                        format!("parent_id {parent} is not a known anchor id"),
                        "Fix: create the missing anchor, or update parent_id to an existing anchor.",
                    );
                }
            }

            let deps = crate::store::anchors::decode_json_string_list(depends_on_json.clone())?;
            for dep in deps {
                if dep == *id {
                    push_issue(
                        &mut issues,
                        "SELF_DEPENDS_ON",
                        "error",
                        id,
                        "depends_on must not include self".to_string(),
                        "Fix: remove the self reference from depends_on.",
                    );
                    continue;
                }
                if anchor_ids.contains(&dep) {
                    continue;
                }
                if let Some(owner) = alias_owner.get(&dep) {
                    if owner == id {
                        push_issue(
                            &mut issues,
                            "SELF_DEPENDS_ON",
                            "error",
                            id,
                            format!("depends_on {dep} resolves back to {id} via alias mapping"),
                            "Fix: remove the self reference from depends_on.",
                        );
                    } else {
                        push_issue(
                            &mut issues,
                            "RELATION_USES_ALIAS",
                            "warning",
                            id,
                            format!("depends_on {dep} is an alias for {owner}"),
                            "Fix: rewrite relations to use canonical ids (aliases are for history preservation).",
                        );
                    }
                    continue;
                }
                push_issue(
                    &mut issues,
                    "UNKNOWN_DEPENDS_ON",
                    "error",
                    id,
                    format!("depends_on {dep} is not a known anchor id"),
                    "Fix: create the missing anchor, or update depends_on to existing anchors.",
                );
            }

            let status = status.trim().to_ascii_lowercase();
            if status != "deprecated" {
                let mut ids = vec![id.clone()];
                if let Some(aliases) = aliases_by_anchor.get(id) {
                    ids.extend(aliases.clone());
                }
                ids.sort();
                ids.dedup();
                let has_links = has_any_anchor_links_tx(&tx, workspace.as_str(), &ids)?;
                if !has_links {
                    push_issue(
                        &mut issues,
                        "ORPHAN_ANCHOR",
                        "warning",
                        id,
                        "anchor has no linked artifacts (no cards tagged with this id or its aliases)".to_string(),
                        "Fix: attach at least one decision/evidence/test to this anchor, or merge/deprecate it.",
                    );
                }
            }
        }

        // Parent cycle detection (single-parent graph).
        let mut reported = std::collections::BTreeSet::<String>::new();
        for id in parent_map.keys() {
            if reported.contains(id) {
                continue;
            }
            let mut seen = std::collections::BTreeMap::<String, usize>::new();
            let mut path = Vec::<String>::new();
            let mut cur = id.clone();
            while let Some(parent) = parent_map.get(&cur) {
                if !anchor_ids.contains(parent) {
                    break;
                }
                if let Some(idx) = seen.get(parent) {
                    // Found a cycle. Report once, anchored to the smallest id in the cycle.
                    let start = *idx;
                    let mut cycle = path[start..].to_vec();
                    cycle.push(parent.clone());
                    cycle.sort();
                    cycle.dedup();
                    let anchor = cycle.first().cloned().unwrap_or_else(|| cur.clone());
                    if reported.insert(anchor.clone()) {
                        push_issue(
                            &mut issues,
                            "PARENT_CYCLE",
                            "error",
                            &anchor,
                            "parent_id cycle detected".to_string(),
                            "Fix: break the cycle by clearing or rewriting parent_id on one of the anchors.",
                        );
                    }
                    break;
                }
                seen.insert(parent.clone(), path.len());
                path.push(parent.clone());
                cur = parent.clone();
            }
        }

        issues.sort_by(|a, b| {
            (
                severity_rank(a.severity.as_str()),
                a.code.to_ascii_lowercase(),
                a.anchor.to_ascii_lowercase(),
                a.message.to_ascii_lowercase(),
            )
                .cmp(&(
                    severity_rank(b.severity.as_str()),
                    b.code.to_ascii_lowercase(),
                    b.anchor.to_ascii_lowercase(),
                    b.message.to_ascii_lowercase(),
                ))
        });

        let has_more = issues.len() > limit;
        issues.truncate(limit);

        tx.commit()?;
        Ok(AnchorsLintResult { issues, has_more })
    }
}
