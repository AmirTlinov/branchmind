#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::OptionalExtension;
use rusqlite::params;

impl SqliteStore {
    pub fn graph_card_open_by_id(
        &mut self,
        workspace: &WorkspaceId,
        card_id: &str,
    ) -> Result<Option<GraphCardOpenResult>, StoreError> {
        let card_id = card_id.trim();
        if card_id.is_empty() {
            return Err(StoreError::InvalidInput("card_id must not be empty"));
        }
        validate_graph_node_id(card_id)?;

        let tx = self.conn.transaction()?;

        let head = tx
            .query_row(
                r#"
                SELECT branch, doc, seq, ts_ms
                FROM graph_node_versions
                WHERE workspace=?1 AND node_id=?2 AND deleted=0
                ORDER BY seq DESC
                LIMIT 1
                "#,
                params![workspace.as_str(), card_id],
                |row| {
                    Ok(GraphCardOpenHead {
                        branch: row.get(0)?,
                        doc: row.get(1)?,
                        seq: row.get(2)?,
                        ts_ms: row.get(3)?,
                    })
                },
            )
            .optional()?;

        let Some(head) = head else {
            tx.commit()?;
            return Ok(None);
        };

        if !branch_exists_tx(&tx, workspace.as_str(), head.branch.as_str())? {
            tx.commit()?;
            return Ok(None);
        }

        let sources = branch_sources_tx(&tx, workspace.as_str(), head.branch.as_str())?;
        let node = graph_node_get_tx(
            &tx,
            workspace.as_str(),
            &sources,
            head.doc.as_str(),
            card_id,
        )?;
        let Some(node) = node else {
            tx.commit()?;
            return Ok(None);
        };

        let mut supports = Vec::<String>::new();
        let mut blocks = Vec::<String>::new();
        let edge_keys = graph_edge_keys_for_node_tx(
            &tx,
            workspace.as_str(),
            &sources,
            head.doc.as_str(),
            card_id,
        )?;
        for key in edge_keys {
            if key.from != card_id {
                continue;
            }
            match key.rel.as_str() {
                "supports" => supports.push(key.to),
                "blocks" => blocks.push(key.to),
                _ => {}
            }
        }
        supports.sort();
        supports.dedup();
        blocks.sort();
        blocks.dedup();

        tx.commit()?;
        Ok(Some(GraphCardOpenResult {
            head,
            node,
            supports,
            blocks,
        }))
    }

    pub fn graph_cards_since(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        since_seq: i64,
        limit: usize,
    ) -> Result<(Vec<GraphNodeRow>, usize), StoreError> {
        let branch = branch.trim();
        let doc = doc.trim();
        if branch.is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if since_seq < 0 {
            return Err(StoreError::InvalidInput("since_seq must be >= 0"));
        }

        let limit = limit.clamp(0, 200) as i64;
        if limit == 0 {
            return Ok((Vec::new(), 0));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let mut stmt = tx.prepare(
            r#"
            WITH candidates AS (
              SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms
              FROM graph_node_versions
              WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq>?4
                AND deleted=0
                AND node_id LIKE 'CARD-%'
            ),
            latest AS (
              SELECT node_id, MAX(seq) AS max_seq
              FROM candidates
              GROUP BY node_id
            )
            SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms
            FROM candidates c
            JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq
            ORDER BY c.seq ASC
            LIMIT ?5
            "#,
        )?;
        let mut rows = stmt.query(params![workspace.as_str(), branch, doc, since_seq, limit])?;

        let mut nodes = Vec::<GraphNodeRow>::new();
        while let Some(row) = rows.next()? {
            let raw_tags: Option<String> = row.get(4)?;
            let deleted: i64 = row.get(7)?;
            if deleted != 0 {
                continue;
            }
            nodes.push(GraphNodeRow {
                id: row.get(0)?,
                node_type: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "card".to_string()),
                title: row.get(2)?,
                text: row.get(3)?,
                tags: decode_tags(raw_tags.as_deref()),
                status: row.get(5)?,
                meta_json: row.get(6)?,
                deleted: false,
                last_seq: row.get(8)?,
                last_ts_ms: row.get(9)?,
            });
        }
        drop(rows);
        drop(stmt);

        let total: i64 = tx.query_row(
            r#"
            SELECT COUNT(DISTINCT node_id)
            FROM graph_node_versions
            WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq>?4
              AND deleted=0
              AND node_id LIKE 'CARD-%'
            "#,
            params![workspace.as_str(), branch, doc, since_seq],
            |row| row.get::<_, i64>(0),
        )?;

        tx.commit()?;
        Ok((nodes, total.max(0) as usize))
    }
}
