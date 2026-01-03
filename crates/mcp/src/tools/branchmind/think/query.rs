#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_query(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let ids = match optional_string_values(args_obj, "ids") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let types = match optional_string_values(args_obj, "types") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_any = match optional_string_values(args_obj, "tags_any") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_all = match optional_string_values(args_obj, "tags_all") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let text = match optional_string(args_obj, "text") {
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

        let (branch, graph_doc) = match self.resolve_think_graph_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        let types =
            types.or_else(|| Some(supported.iter().map(|v| v.to_string()).collect::<Vec<_>>()));

        let slice = match self.store.graph_query(
            &workspace,
            &branch,
            &graph_doc,
            bm_storage::GraphQueryRequest {
                ids,
                types,
                status,
                tags_any,
                tags_all,
                text,
                cursor: None,
                limit,
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

        let cards = graph_nodes_to_cards(slice.nodes);
        let cards_total = cards.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "cards": cards,
            "pagination": {
                "cursor": Value::Null,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": cards.len()
            },
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let before = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, cards_truncated) = enforce_graph_list_budget(&mut result, "cards", limit);
            truncated |= cards_truncated;
            let after = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if after < before {
                let next_cursor = result
                    .get("cards")
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
                        Value::Number(serde_json::Number::from(after as u64)),
                    );
                };
            }
            if after == 0
                && cards_total > 0
                && ensure_minimal_list_at(&mut result, &["cards"], cards_total, "cards")
            {
                truncated = true;
                minimal = true;
                if let Some(pagination) =
                    result.get_mut("pagination").and_then(|v| v.as_object_mut())
                {
                    pagination.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(cards_total as u64)),
                    );
                    pagination.insert("has_more".to_string(), Value::Bool(true));
                }
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        let retained = retain_one_at(value, &["cards"], true);
                        if retained {
                            changed = true;
                            refresh_pagination_count(value, &["cards"], &["pagination"]);
                        }
                    }
                    if json_len_chars(value) > limit {
                        let ensured =
                            ensure_minimal_list_at(value, &["cards"], cards_total, "cards");
                        if ensured {
                            changed = true;
                            if let Some(pagination) =
                                value.get_mut("pagination").and_then(|v| v.as_object_mut())
                            {
                                pagination.insert(
                                    "count".to_string(),
                                    Value::Number(serde_json::Number::from(cards_total as u64)),
                                );
                                pagination.insert("has_more".to_string(), Value::Bool(true));
                            }
                        }
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(
                            value,
                            &["pagination"],
                            &["next_cursor", "has_more", "count"],
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["cards"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["pagination"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_query", result)
        } else {
            ai_ok_with_warnings("think_query", result, warnings, Vec::new())
        }
    }
}
