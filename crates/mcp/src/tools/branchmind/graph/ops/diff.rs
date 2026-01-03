#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_diff(&mut self, args: Value) -> Value {
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
        let to = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let to_exists = match self.store.branch_exists(&workspace, &to) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !to_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown to-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let slice = match self
            .store
            .graph_diff(&workspace, &from, &to, &doc, cursor, limit)
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

        let changes = slice
            .changes
            .into_iter()
            .map(|c| match c {
                bm_storage::GraphDiffChange::Node { to: n } => {
                    let id = n.id.clone();
                    json!({
                        "kind": "node",
                        "id": id,
                        "to": {
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
                        }
                    })
                }
                bm_storage::GraphDiffChange::Edge { to: e } => {
                    let from = e.from.clone();
                    let rel = e.rel.clone();
                    let to = e.to.clone();
                    json!({
                        "kind": "edge",
                        "key": { "from": from, "rel": rel, "to": to },
                        "to": {
                            "from": e.from,
                            "rel": e.rel,
                            "to": e.to,
                            "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                            "deleted": e.deleted,
                            "last_seq": e.last_seq,
                            "last_ts_ms": e.last_ts_ms
                        }
                    })
                }
            })
            .collect::<Vec<_>>();

        let change_count = changes.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "from": from,
            "to": to,
            "doc": doc,
            "changes": changes,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": change_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before = result
                .get("changes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "changes", limit);
            let after = result
                .get("changes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            set_truncated_flag(&mut result, truncated);
            if after < before {
                let next_cursor = result
                    .get("changes")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("to"))
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
                        Value::Number(serde_json::Number::from(after as u64)),
                    );
                };
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "changes", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("graph_diff", result)
    }
}
