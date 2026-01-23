#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use rusqlite::{Connection, OptionalExtension, params};

// Backfill: historical databases may contain anchor_links created before anchors existed or before
// auto-registration was implemented. Those anchors should still be listable/openable without the
// agent having to “re-touch” the history.
//
// This migration is intentionally:
// - idempotent (INSERT/ensure semantics),
// - deterministic (stable ordering),
// - bounded in behavior (does not scan graph content; only uses the anchor_links index).
pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    // Fast path: nothing to do.
    let has_missing: Option<()> = conn
        .query_row(
            r#"
            SELECT 1
            FROM anchor_links al
            LEFT JOIN anchors a
              ON a.workspace=al.workspace AND a.id=al.anchor_id
            LEFT JOIN anchor_aliases aa
              ON aa.workspace=al.workspace AND aa.alias_id=al.anchor_id
            WHERE a.id IS NULL
              AND aa.alias_id IS NULL
            LIMIT 1
            "#,
            [],
            |_| Ok(()),
        )
        .optional()?;
    if has_missing.is_none() {
        return Ok(());
    }

    backfill_missing_anchors_from_links(conn)
}

fn backfill_missing_anchors_from_links(conn: &Connection) -> Result<(), StoreError> {
    // For each anchor_id present in anchor_links but missing from anchors, create a minimal anchor
    // record (component/active) with timestamps derived from the history (max last_ts_ms).
    //
    // We exclude alias ids: aliases remain redirects and should not become anchors.
    let mut stmt = conn.prepare(
        r#"
        SELECT
          al.workspace,
          al.anchor_id,
          MAX(al.last_ts_ms) AS last_ts_ms
        FROM anchor_links al
        LEFT JOIN anchors a
          ON a.workspace=al.workspace AND a.id=al.anchor_id
        LEFT JOIN anchor_aliases aa
          ON aa.workspace=al.workspace AND aa.alias_id=al.anchor_id
        WHERE a.id IS NULL
          AND aa.alias_id IS NULL
        GROUP BY al.workspace, al.anchor_id
        ORDER BY al.workspace ASC, al.anchor_id ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let workspace: String = row.get(0)?;
        let anchor_id: String = row.get(1)?;
        let last_ts_ms: i64 = row.get(2)?;

        // Deterministic timestamps: based on observed history, not wall-clock.
        let id = crate::store::anchors::normalize_anchor_id(&anchor_id)?;
        let title = title_from_anchor_id(&id);
        let ts_ms = last_ts_ms.max(0);

        // Idempotent: anchor may have been created concurrently (or in a prior run).
        let _ = conn.execute(
            r#"
            INSERT OR IGNORE INTO anchors(
              workspace, id, title, kind, description, refs_json, parent_id, depends_on_json, status, created_at_ms, updated_at_ms
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                workspace.as_str(),
                id.as_str(),
                title,
                "component",
                Option::<String>::None,
                "[]",
                Option::<String>::None,
                "[]",
                "active",
                ts_ms,
                ts_ms,
            ],
        )?;
    }
    Ok(())
}

fn title_from_anchor_id(anchor_id: &str) -> String {
    const MAP_ANCHOR_ID_PREFIX: &str = "a:";
    const MAX_ANCHOR_TITLE_LEN: usize = 120;

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
