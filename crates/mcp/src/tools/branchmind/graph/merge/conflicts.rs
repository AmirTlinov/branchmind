#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_conflicts(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let into = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
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
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (conflicts, next_cursor, has_more) = match self.store.graph_conflicts_list(
            &workspace,
            &into,
            &doc,
            status.as_deref(),
            cursor,
            limit,
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

        let conflicts = conflicts
            .into_iter()
            .map(|c| {
                json!({
                    "conflict_id": c.conflict_id,
                    "kind": c.kind,
                    "key": c.key,
                    "status": c.status,
                    "created_at_ms": c.created_at_ms
                })
            })
            .collect::<Vec<_>>();

        let conflict_count = conflicts.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "into": into,
            "doc": doc,
            "conflicts": conflicts,
            "pagination": {
                "cursor": cursor,
                "next_cursor": next_cursor,
                "has_more": has_more,
                "limit": limit,
                "count": conflict_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before = result
                .get("conflicts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "conflicts", limit);
            let after = result
                .get("conflicts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            set_truncated_flag(&mut result, truncated);
            if after < before {
                let next_cursor = result
                    .get("conflicts")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("created_at_ms"))
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
                let (_used2, truncated2) =
                    enforce_graph_list_budget(&mut result, "conflicts", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("graph_conflicts", result)
    }

    pub(crate) fn tool_branchmind_graph_conflict_show(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let conflict_id = match require_string(args_obj, "conflict_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let detail = match self.store.graph_conflict_show(&workspace, &conflict_id) {
            Ok(v) => v,
            Err(StoreError::UnknownConflict) => return ai_error("UNKNOWN_ID", "Unknown conflict"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let detail_json = Self::conflict_detail_to_json(&detail);

        ai_ok(
            "graph_conflict_show",
            json!({
                "workspace": workspace.as_str(),
                "conflict": detail_json
            }),
        )
    }

    pub(crate) fn tool_branchmind_graph_conflict_resolve(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let conflict_id = match require_string(args_obj, "conflict_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let resolution = match require_string(args_obj, "resolution") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let resolved =
            match self
                .store
                .graph_conflict_resolve(&workspace, &conflict_id, &resolution)
            {
                Ok(v) => v,
                Err(StoreError::UnknownConflict) => {
                    return ai_error("UNKNOWN_ID", "Unknown conflict");
                }
                Err(StoreError::ConflictAlreadyResolved) => {
                    return ai_error("INVALID_INPUT", "Conflict already resolved");
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        ai_ok(
            "graph_conflict_resolve",
            json!({
                "workspace": workspace.as_str(),
                "conflict_id": resolved.conflict_id,
                "status": resolved.status,
                "applied": resolved.applied
            }),
        )
    }
}
