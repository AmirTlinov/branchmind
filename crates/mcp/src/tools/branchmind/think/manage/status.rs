#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_set_status(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let status = match require_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let targets = match parse_string_values(args_obj.get("targets"), "targets") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if targets.is_empty() {
            return ai_error("INVALID_INPUT", "targets must not be empty");
        }
        let message = match optional_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_json = match optional_object_as_json_string(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, graph_doc) = match self.resolve_think_graph_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let slice = match self.store.graph_query(
            &workspace,
            &branch,
            &graph_doc,
            bm_storage::GraphQueryRequest {
                ids: Some(targets.clone()),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: targets.len(),
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        if slice.nodes.len() != targets.len() {
            return ai_error("UNKNOWN_ID", "One or more targets not found");
        }

        let mut ops = Vec::with_capacity(slice.nodes.len());
        for node in slice.nodes {
            let merged_meta = merge_meta_with_message(
                node.meta_json.as_deref(),
                message.clone(),
                meta_json.clone(),
            );
            ops.push(bm_storage::GraphOp::NodeUpsert(
                bm_storage::GraphNodeUpsert {
                    id: node.id,
                    node_type: node.node_type,
                    title: node.title,
                    text: node.text,
                    tags: node.tags,
                    status: Some(status.clone()),
                    meta_json: merged_meta,
                },
            ));
        }

        let applied = match self
            .store
            .graph_apply_ops(&workspace, &branch, &graph_doc, ops)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "think_set_status",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "graph_doc": graph_doc,
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
