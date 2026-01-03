#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn graph_diff(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        to_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<GraphDiffSlice, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if to_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("to_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let scan_limit = (limit * 5).clamp(limit, 1000);
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), to_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let from_sources = branch_sources_tx(&tx, workspace.as_str(), from_branch)?;
        let to_sources = branch_sources_tx(&tx, workspace.as_str(), to_branch)?;

        let candidates = graph_diff_candidates_tx(
            &tx,
            workspace.as_str(),
            &to_sources,
            doc,
            before_seq,
            scan_limit + 1,
        )?;

        let mut changes = Vec::new();
        let mut scanned = 0usize;

        let mut node_ids = Vec::new();
        let mut edge_keys = Vec::new();
        for candidate in candidates.iter().take(scan_limit as usize) {
            match candidate {
                GraphDiffCandidate::Node { to, .. } => node_ids.push(to.id.clone()),
                GraphDiffCandidate::Edge { key, .. } => edge_keys.push(key.clone()),
            }
        }

        let from_nodes =
            graph_nodes_get_map_tx(&tx, workspace.as_str(), &from_sources, doc, &node_ids, true)?;
        let from_edges = graph_edges_get_map_tx(
            &tx,
            workspace.as_str(),
            &from_sources,
            doc,
            &edge_keys,
            true,
        )?;

        for candidate in candidates.iter().take(scan_limit as usize) {
            if changes.len() as i64 >= limit {
                break;
            }
            scanned += 1;
            match candidate {
                GraphDiffCandidate::Node { to, .. } => {
                    let from = from_nodes.get(&to.id);
                    if !graph_node_semantic_eq(from, Some(to)) {
                        changes.push(GraphDiffChange::Node { to: to.clone() });
                    }
                }
                GraphDiffCandidate::Edge { key, to, .. } => {
                    let from = from_edges.get(key);
                    if !graph_edge_semantic_eq(from, Some(to)) {
                        changes.push(GraphDiffChange::Edge { to: to.clone() });
                    }
                }
            }
        }

        let has_more = candidates.len() > scanned;
        let next_cursor = if has_more {
            candidates
                .get(scanned.saturating_sub(1))
                .map(|c| c.last_seq())
        } else {
            None
        };

        tx.commit()?;
        Ok(GraphDiffSlice {
            changes,
            next_cursor,
            has_more,
        })
    }
}
