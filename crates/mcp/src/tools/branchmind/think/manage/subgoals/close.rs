#![forbid(unsafe_code)]

use super::super::super::super::graph::ThinkCardCommitInternalArgs;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_subgoal_close(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let subgoal_id = match require_string(args_obj, "subgoal_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let reference = match optional_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_graph_doc = match optional_string(args_obj, "parent_graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_trace_doc = match optional_string(args_obj, "parent_trace_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let return_card_value = args_obj.get("return_card").cloned();

        let branch = match reference {
            Some(v) => v,
            None => match require_checkout_branch(&mut self.store, &workspace) {
                Ok(v) => v,
                Err(resp) => return resp,
            },
        };
        if !self
            .store
            .branch_exists(&workspace, &branch)
            .unwrap_or(false)
        {
            return unknown_branch_error(&workspace);
        }
        let parent_graph_doc = parent_graph_doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
        let parent_trace_doc = parent_trace_doc.unwrap_or_else(|| DEFAULT_TRACE_DOC.to_string());

        let slice = match self.store.graph_query(
            &workspace,
            &branch,
            &parent_graph_doc,
            bm_storage::GraphQueryRequest {
                ids: Some(vec![subgoal_id.clone()]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 1,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => return unknown_branch_error(&workspace),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let Some(node) = slice.nodes.into_iter().next() else {
            return ai_error("UNKNOWN_ID", "Unknown subgoal id");
        };

        let mut ops = vec![bm_storage::GraphOp::NodeUpsert(
            bm_storage::GraphNodeUpsert {
                id: node.id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                text: node.text.clone(),
                tags: node.tags.clone(),
                status: Some("closed".to_string()),
                meta_json: node.meta_json.clone(),
            },
        )];

        let mut return_card_id: Option<String> = None;
        if let Some(return_card_value) = return_card_value {
            let parsed = match parse_think_card(&workspace, return_card_value) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let (card_id, _result) =
                match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
                    workspace: &workspace,
                    branch: &branch,
                    trace_doc: &parent_trace_doc,
                    graph_doc: &parent_graph_doc,
                    parsed,
                    supports: &[],
                    blocks: &[],
                }) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            return_card_id = Some(card_id.clone());
            ops.push(bm_storage::GraphOp::EdgeUpsert(
                bm_storage::GraphEdgeUpsert {
                    from: node.id,
                    rel: "return".to_string(),
                    to: card_id,
                    meta_json: None,
                },
            ));
        }

        let applied = match self
            .store
            .graph_apply_ops(&workspace, &branch, &parent_graph_doc, ops)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => return unknown_branch_error(&workspace),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "think_subgoal_close",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "graph_doc": parent_graph_doc,
                "subgoal_id": subgoal_id,
                "return_card_id": return_card_id,
                "applied": {
                    "nodes_upserted": applied.nodes_upserted,
                    "nodes_deleted": applied.nodes_deleted,
                    "edges_upserted": applied.edges_upserted,
                    "edges_deleted": applied.edges_deleted,
                    "last_seq": applied.last_seq,
                    "last_ts_ms": applied.last_ts_ms
                }
            }),
        )
    }
}
