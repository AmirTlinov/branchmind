#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_link(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let rel = match require_string(args_obj, "rel") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
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

        let edge_meta = merge_meta_with_message(None, message, meta_json);
        let applied = match self.store.graph_apply_ops(
            &workspace,
            &branch,
            &graph_doc,
            vec![bm_storage::GraphOp::EdgeUpsert(
                bm_storage::GraphEdgeUpsert {
                    from,
                    rel,
                    to,
                    meta_json: edge_meta,
                },
            )],
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

        ai_ok(
            "think_link",
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
