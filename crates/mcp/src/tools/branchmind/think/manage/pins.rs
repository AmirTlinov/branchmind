#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_pin(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let targets = match parse_string_values(args_obj.get("targets"), "targets") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if targets.is_empty() {
            return ai_error("INVALID_INPUT", "targets must not be empty");
        }
        let pinned = match optional_bool(args_obj, "pinned") {
            Ok(v) => v.unwrap_or(true),
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
            let mut tags = node.tags.clone();
            if pinned {
                if !tags.iter().any(|t| t == PIN_TAG) {
                    tags.push(PIN_TAG.to_string());
                }
            } else {
                tags.retain(|t| t != PIN_TAG);
            }
            ops.push(bm_storage::GraphOp::NodeUpsert(
                bm_storage::GraphNodeUpsert {
                    id: node.id,
                    node_type: node.node_type,
                    title: node.title,
                    text: node.text,
                    tags,
                    status: node.status,
                    meta_json: node.meta_json,
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
            "think_pin",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "graph_doc": graph_doc,
                "pinned": pinned,
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

    pub(crate) fn tool_branchmind_think_pins(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
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
        let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();

        let slice = match self.store.graph_query(
            &workspace,
            &branch,
            &graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(types),
                status: None,
                tags_any: None,
                tags_all: Some(vec![PIN_TAG.to_string()]),
                text: None,
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

        let pins = graph_nodes_to_cards(slice.nodes);
        let pins_count = pins.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "pins": pins,
            "pagination": {
                "cursor": Value::Null,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": pins_count
            },
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let (_used, pins_truncated) = enforce_graph_list_budget(&mut result, "pins", limit);
            truncated |= pins_truncated;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        let retained = retain_one_at(value, &["pins"], true);
                        if retained {
                            changed = true;
                            refresh_pagination_count(value, &["pins"], &["pagination"]);
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
                        changed |= drop_fields_at(value, &[], &["pins"]);
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
            ai_ok("think_pins", result)
        } else {
            ai_ok_with_warnings("think_pins", result, warnings, Vec::new())
        }
    }
}
