#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_nominal_merge(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let candidate_ids = match optional_string_values(args_obj, "candidate_ids") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit_candidates = match optional_usize(args_obj, "limit_candidates") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let limit_groups = match optional_usize(args_obj, "limit_groups") {
            Ok(v) => v.unwrap_or(10),
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

        let nodes = match candidate_ids {
            Some(ids) => {
                if ids.is_empty() {
                    Vec::new()
                } else {
                    let slice = match self.store.graph_query(
                        &workspace,
                        &branch,
                        &graph_doc,
                        bm_storage::GraphQueryRequest {
                            ids: Some(ids.clone()),
                            types: None,
                            status: None,
                            tags_any: None,
                            tags_all: None,
                            text: None,
                            cursor: None,
                            limit: ids.len(),
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
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    if slice.nodes.len() != ids.len() {
                        return ai_error("UNKNOWN_ID", "One or more candidates not found");
                    }
                    slice.nodes
                }
            }
            None => {
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
                        tags_all: None,
                        text: None,
                        cursor: None,
                        limit: limit_candidates,
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
                slice.nodes
            }
        };

        let mut groups: std::collections::BTreeMap<String, Vec<bm_storage::GraphNode>> =
            std::collections::BTreeMap::new();
        for node in nodes {
            let key = format!(
                "{}|{}|{}",
                node.node_type,
                node.title.clone().unwrap_or_default(),
                node.text.clone().unwrap_or_default()
            );
            groups.entry(key).or_default().push(node);
        }

        let mut ops: Vec<bm_storage::GraphOp> = Vec::new();
        let mut merged_groups: Vec<Value> = Vec::new();
        for (_key, mut group) in groups {
            if group.len() < 2 {
                continue;
            }
            group.sort_by_key(|n| std::cmp::Reverse(n.last_seq));
            let canonical = group[0].clone();
            let mut merged_ids = Vec::new();
            for dup in group.iter().skip(1) {
                merged_ids.push(dup.id.clone());
                ops.push(bm_storage::GraphOp::EdgeUpsert(
                    bm_storage::GraphEdgeUpsert {
                        from: dup.id.clone(),
                        rel: "dedup".to_string(),
                        to: canonical.id.clone(),
                        meta_json: None,
                    },
                ));
                ops.push(bm_storage::GraphOp::NodeUpsert(
                    bm_storage::GraphNodeUpsert {
                        id: dup.id.clone(),
                        node_type: dup.node_type.clone(),
                        title: dup.title.clone(),
                        text: dup.text.clone(),
                        tags: dup.tags.clone(),
                        status: Some("merged".to_string()),
                        meta_json: dup.meta_json.clone(),
                    },
                ));
            }
            merged_groups.push(json!({
                "canonical_id": canonical.id,
                "merged_ids": merged_ids
            }));
            if merged_groups.len() >= limit_groups {
                break;
            }
        }

        let applied = if ops.is_empty() {
            None
        } else {
            Some(
                match self
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
                },
            )
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "merged_groups": merged_groups,
            "applied": applied.as_ref().map(|applied| json!({
                "nodes_upserted": applied.nodes_upserted,
                "nodes_deleted": applied.nodes_deleted,
                "edges_upserted": applied.edges_upserted,
                "edges_deleted": applied.edges_deleted,
                "last_seq": applied.last_seq,
                "last_ts_ms": applied.last_ts_ms
            })).unwrap_or(Value::Null),
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["merged_groups"], limit, false);
            }
            if json_len_chars(&result) > limit {
                let merged_total = result
                    .get("merged_groups")
                    .and_then(|v| v.as_array())
                    .map(|v| v.len())
                    .unwrap_or(0);
                if merged_total > 0
                    && ensure_minimal_list_at(
                        &mut result,
                        &["merged_groups"],
                        merged_total,
                        "merged_groups",
                    )
                {
                    truncated = true;
                    minimal = true;
                }
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["applied"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["merged_groups"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_nominal_merge", result)
        } else {
            ai_ok_with_warnings("think_nominal_merge", result, warnings, Vec::new())
        }
    }
}
