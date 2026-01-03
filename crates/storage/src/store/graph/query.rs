#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn graph_query(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        request: GraphQueryRequest,
    ) -> Result<GraphQuerySlice, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let cursor = request.cursor.unwrap_or(i64::MAX);
        let limit = request.limit.clamp(1, 200) as i64;
        let edges_limit = request.edges_limit.clamp(0, 1000) as i64;
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

        let mut nodes = graph_nodes_query_tx(
            &tx,
            workspace.as_str(),
            &sources,
            doc,
            cursor,
            limit,
            &request,
        )?;

        let has_more = nodes.len() as i64 > limit;
        if has_more {
            nodes.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            nodes.last().map(|n| n.last_seq)
        } else {
            None
        };

        let mut edges = Vec::new();
        if request.include_edges && !nodes.is_empty() && edges_limit > 0 {
            let node_ids = nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
            edges = graph_edges_for_nodes_tx(
                &tx,
                workspace.as_str(),
                &sources,
                doc,
                &node_ids,
                edges_limit,
            )?;
        }

        tx.commit()?;
        Ok(GraphQuerySlice {
            nodes,
            edges,
            next_cursor,
            has_more,
        })
    }
}
