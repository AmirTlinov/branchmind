#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_query(&mut self, args: Value) -> Value {
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

        let ids = match optional_string_array(args_obj, "ids") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let types = match optional_string_array(args_obj, "types") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_any = match optional_string_array(args_obj, "tags_any") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_all = match optional_string_array(args_obj, "tags_all") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let text = match optional_string(args_obj, "text") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let include_edges = match optional_bool(args_obj, "include_edges") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let edges_limit = match optional_usize(args_obj, "edges_limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let request = bm_storage::GraphQueryRequest {
            ids,
            types,
            status,
            tags_any,
            tags_all,
            text,
            cursor,
            limit,
            include_edges,
            edges_limit,
        };

        let slice = match self.store.graph_query(&workspace, &branch, &doc, request) {
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

        let nodes = slice
            .nodes
            .into_iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "type": n.node_type,
                    "title": n.title,
                    "text": n.text,
                    "status": n.status,
                    "tags": n.tags,
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            })
            .collect::<Vec<_>>();
        let edges = slice
            .edges
            .into_iter()
            .map(|e| {
                json!({
                    "from": e.from,
                    "rel": e.rel,
                    "to": e.to,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": e.deleted,
                    "last_seq": e.last_seq,
                    "last_ts_ms": e.last_ts_ms
                })
            })
            .collect::<Vec<_>>();

        let node_count = nodes.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "nodes": nodes,
            "edges": edges,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": node_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before_nodes = result
                .get("nodes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_query_budget(&mut result, limit);
            let after_nodes = result
                .get("nodes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            set_truncated_flag(&mut result, truncated);
            if after_nodes < before_nodes {
                let next_cursor = result
                    .get("nodes")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("last_seq"))
                    .and_then(|v| v.as_i64())
                    .map(serde_json::Number::from);
                if let (Some(next_cursor), Some(pagination)) = (
                    next_cursor,
                    result.get_mut("pagination").and_then(|v| v.as_object_mut()),
                ) {
                    pagination.insert("next_cursor".to_string(), Value::Number(next_cursor));
                    pagination.insert("has_more".to_string(), Value::Bool(true));
                    pagination.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(after_nodes as u64)),
                    );
                };
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_query_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("graph_query", result)
    }
}
