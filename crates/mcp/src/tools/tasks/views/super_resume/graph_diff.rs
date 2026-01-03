#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(super) fn build_resume_super_graph_diff_payload(
        &mut self,
        workspace: &WorkspaceId,
        reasoning: &bm_storage::ReasoningRefRow,
        reasoning_branch_missing: bool,
        graph_diff_cursor: Option<i64>,
        graph_diff_limit: usize,
        warnings: &mut Vec<Value>,
    ) -> Result<Option<Value>, Value> {
        if reasoning_branch_missing {
            warnings.push(warning(
                "GRAPH_DIFF_UNAVAILABLE",
                "Graph diff unavailable because the reasoning branch is missing.",
                "Seed reasoning via think_pipeline or switch read_only=false to create refs.",
            ));
            return Ok(Some(json!({
                "available": false,
                "reason": "branch_missing",
                "branch": reasoning.branch,
                "doc": reasoning.graph_doc
            })));
        }

        match self.store.branch_base_info(workspace, &reasoning.branch) {
            Ok(Some((base_branch, _base_seq))) => {
                let diff_slice = match self.store.graph_diff(
                    workspace,
                    &base_branch,
                    &reasoning.branch,
                    &reasoning.graph_doc,
                    graph_diff_cursor,
                    graph_diff_limit,
                ) {
                    Ok(v) => Some(v),
                    Err(StoreError::UnknownBranch) => {
                        warnings.push(warning(
                            "GRAPH_DIFF_UNAVAILABLE",
                            "Graph diff unavailable because the reasoning branch is missing.",
                            "Seed reasoning via think_pipeline or switch read_only=false to create refs.",
                        ));
                        return Ok(Some(json!({
                            "available": false,
                            "reason": "branch_missing",
                            "branch": reasoning.branch,
                            "doc": reasoning.graph_doc
                        })));
                    }
                    Err(StoreError::InvalidInput(msg)) => {
                        return Err(ai_error("INVALID_INPUT", msg));
                    }
                    Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                };

                let Some(diff_slice) = diff_slice else {
                    return Ok(None);
                };

                let mut nodes_changed = 0usize;
                let mut edges_changed = 0usize;
                for change in diff_slice.changes.iter() {
                    match change {
                        bm_storage::GraphDiffChange::Node { .. } => nodes_changed += 1,
                        bm_storage::GraphDiffChange::Edge { .. } => edges_changed += 1,
                    }
                }

                Ok(Some(json!({
                    "available": true,
                    "branch": reasoning.branch,
                    "base": base_branch,
                    "base_source": "branch_base",
                    "doc": reasoning.graph_doc,
                    "summary": {
                        "nodes_changed": nodes_changed,
                        "edges_changed": edges_changed,
                        "total": nodes_changed + edges_changed,
                        "partial": diff_slice.has_more
                    },
                    "pagination": {
                        "cursor": graph_diff_cursor.map(|v| Value::Number(serde_json::Number::from(v))).unwrap_or(Value::Null),
                        "next_cursor": diff_slice.next_cursor,
                        "has_more": diff_slice.has_more,
                        "limit": graph_diff_limit,
                        "count": diff_slice.changes.len()
                    }
                })))
            }
            Ok(None) => {
                let checkout_branch = match self.store.branch_checkout_get(workspace) {
                    Ok(v) => v,
                    Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                };
                let checkout_branch = checkout_branch
                    .filter(|b| b != &reasoning.branch)
                    .map(|b| b.to_string());

                let Some(base_branch) = checkout_branch else {
                    return Ok(Some(json!({
                        "available": false,
                        "reason": "no_base",
                        "branch": reasoning.branch,
                        "doc": reasoning.graph_doc
                    })));
                };

                let diff_slice = match self.store.graph_diff(
                    workspace,
                    &base_branch,
                    &reasoning.branch,
                    &reasoning.graph_doc,
                    graph_diff_cursor,
                    graph_diff_limit,
                ) {
                    Ok(v) => Some(v),
                    Err(StoreError::UnknownBranch) => {
                        warnings.push(warning(
                            "GRAPH_DIFF_UNAVAILABLE",
                            "Graph diff unavailable because the reasoning branch is missing.",
                            "Seed reasoning via think_pipeline or switch read_only=false to create refs.",
                        ));
                        return Ok(Some(json!({
                            "available": false,
                            "reason": "branch_missing",
                            "branch": reasoning.branch,
                            "doc": reasoning.graph_doc
                        })));
                    }
                    Err(StoreError::InvalidInput(msg)) => {
                        return Err(ai_error("INVALID_INPUT", msg));
                    }
                    Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                };

                let Some(diff_slice) = diff_slice else {
                    return Ok(None);
                };

                let mut nodes_changed = 0usize;
                let mut edges_changed = 0usize;
                for change in diff_slice.changes.iter() {
                    match change {
                        bm_storage::GraphDiffChange::Node { .. } => nodes_changed += 1,
                        bm_storage::GraphDiffChange::Edge { .. } => edges_changed += 1,
                    }
                }

                Ok(Some(json!({
                    "available": true,
                    "branch": reasoning.branch,
                    "base": base_branch,
                    "base_source": "checkout",
                    "doc": reasoning.graph_doc,
                    "summary": {
                        "nodes_changed": nodes_changed,
                        "edges_changed": edges_changed,
                        "total": nodes_changed + edges_changed,
                        "partial": diff_slice.has_more
                    },
                    "pagination": {
                        "cursor": graph_diff_cursor.map(|v| Value::Number(serde_json::Number::from(v))).unwrap_or(Value::Null),
                        "next_cursor": diff_slice.next_cursor,
                        "has_more": diff_slice.has_more,
                        "limit": graph_diff_limit,
                        "count": diff_slice.changes.len()
                    }
                })))
            }
            Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
    }
}
