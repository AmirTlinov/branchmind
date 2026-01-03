#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn graph_validate(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        max_errors: usize,
    ) -> Result<GraphValidateResult, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let max_errors = max_errors.clamp(1, 500);
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
        let nodes = graph_nodes_all_tx(&tx, workspace.as_str(), &sources, doc, false)?;
        let edges = graph_edges_all_tx(&tx, workspace.as_str(), &sources, doc, false)?;

        use std::collections::HashSet;
        let mut node_set = HashSet::new();
        for node in nodes.iter() {
            if !node.deleted {
                node_set.insert(node.id.as_str());
            }
        }

        let mut errors = Vec::new();
        for edge in edges.iter() {
            if edge.deleted {
                continue;
            }
            if !node_set.contains(edge.from.as_str()) || !node_set.contains(edge.to.as_str()) {
                let key = format!("{}|{}|{}", edge.from, edge.rel, edge.to);
                errors.push(GraphValidateError {
                    code: "EDGE_ENDPOINT_MISSING",
                    message: "edge endpoint is missing or deleted".to_string(),
                    kind: "edge",
                    key,
                });
                if errors.len() >= max_errors {
                    break;
                }
            }
        }

        tx.commit()?;
        Ok(GraphValidateResult {
            ok: errors.is_empty(),
            nodes: nodes.into_iter().filter(|n| !n.deleted).count(),
            edges: edges.into_iter().filter(|e| !e.deleted).count(),
            errors,
        })
    }
}
