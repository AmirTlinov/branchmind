#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_apply(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, doc) =
            match super::resolve_target_or_branch_doc(self, &workspace, target, branch, doc) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let ops_value = args_obj.get("ops").cloned().unwrap_or(Value::Null);
        let Some(ops_array) = ops_value.as_array() else {
            return ai_error("INVALID_INPUT", "ops must be an array");
        };
        if ops_array.is_empty() {
            return ai_error("INVALID_INPUT", "ops must not be empty");
        }
        let mut ops = Vec::with_capacity(ops_array.len());
        for op_value in ops_array {
            let Some(op_obj) = op_value.as_object() else {
                return ai_error("INVALID_INPUT", "ops[] must be an array of objects");
            };
            let op_name = op_obj.get("op").and_then(|v| v.as_str()).unwrap_or("");
            match op_name {
                "node_upsert" => {
                    let id = match require_string(op_obj, "id") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let node_type = match require_string(op_obj, "type") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let title = match optional_string(op_obj, "title") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let text = match optional_string(op_obj, "text") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let status = match optional_string(op_obj, "status") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let tags = match optional_string_array(op_obj, "tags") {
                        Ok(v) => v.unwrap_or_default(),
                        Err(resp) => return resp,
                    };
                    let meta_json = match optional_object_as_json_string(op_obj, "meta") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::NodeUpsert(
                        bm_storage::GraphNodeUpsert {
                            id,
                            node_type,
                            title,
                            text,
                            tags,
                            status,
                            meta_json,
                        },
                    ));
                }
                "node_delete" => {
                    let id = match require_string(op_obj, "id") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::NodeDelete { id });
                }
                "edge_upsert" => {
                    let from = match require_string(op_obj, "from") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let rel = match require_string(op_obj, "rel") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let to = match require_string(op_obj, "to") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let meta_json = match optional_object_as_json_string(op_obj, "meta") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::EdgeUpsert(
                        bm_storage::GraphEdgeUpsert {
                            from,
                            rel,
                            to,
                            meta_json,
                        },
                    ));
                }
                "edge_delete" => {
                    let from = match require_string(op_obj, "from") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let rel = match require_string(op_obj, "rel") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let to = match require_string(op_obj, "to") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::EdgeDelete { from, rel, to });
                }
                _ => {
                    return ai_error(
                        "INVALID_INPUT",
                        "ops[].op must be one of: node_upsert|node_delete|edge_upsert|edge_delete",
                    );
                }
            }
        }

        let applied = match self.store.graph_apply_ops(&workspace, &branch, &doc, ops) {
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
            "graph_apply",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "doc": doc,
                "applied": {
                    "nodes_upserted": applied.nodes_upserted,
                    "nodes_deleted": applied.nodes_deleted,
                    "edges_upserted": applied.edges_upserted,
                    "edges_deleted": applied.edges_deleted
                },
                "last_seq": applied.last_seq,
                "last_ts_ms": applied.last_ts_ms
            }),
        )
    }
}
