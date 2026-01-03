#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_context(&mut self, args: Value) -> Value {
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
        let branch_override = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let graph_doc = match optional_string(args_obj, "graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if let Err(resp) = ensure_nonempty_doc(&graph_doc, "graph_doc") {
            return resp;
        }

        let scope = match self.resolve_reasoning_scope(
            &workspace,
            ReasoningScopeInput {
                target,
                branch: branch_override,
                notes_doc: None,
                graph_doc,
                trace_doc: None,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let branch = scope.branch;
        let graph_doc = scope.graph_doc;

        let limit_cards = match optional_usize(args_obj, "limit_cards") {
            Ok(v) => v.unwrap_or(30),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
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
                tags_all: None,
                text: None,
                cursor: None,
                limit: limit_cards,
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

        let mut by_type = std::collections::BTreeMap::<String, u64>::new();
        for n in &slice.nodes {
            *by_type.entry(n.node_type.clone()).or_insert(0) += 1;
        }

        let cards = slice
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

        let cards_total = cards.len();
        let stats_by_type = by_type.clone();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "stats": {
                "cards": cards.len(),
                "by_type": by_type
            },
            "cards": cards,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |=
                    compact_card_fields_at(&mut result, &["cards"], 180, true, true, false);
            }
            let trimmed = trim_array_to_budget(&mut result, &["cards"], limit, false);
            truncated |= trimmed;
            if trimmed {
                if ensure_minimal_list_at(&mut result, &["cards"], cards_total, "cards") {
                    minimal = true;
                    set_card_stats(&mut result, cards_total, &stats_by_type);
                } else {
                    recompute_card_stats(&mut result, "cards");
                }
            }
            if json_len_chars(&result) > limit && compact_stats_by_type(&mut result) {
                truncated = true;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= compact_card_fields_at(value, &["cards"], 120, true, true, true);
                    }
                    if json_len_chars(value) > limit {
                        changed |= minimalize_cards_at(value, &["cards"]);
                    }
                    if json_len_chars(value) > limit {
                        let retained = retain_one_at(value, &["cards"], true);
                        if retained {
                            changed = true;
                            recompute_card_stats(value, "cards");
                        }
                    }
                    if json_len_chars(value) > limit {
                        let ensured =
                            ensure_minimal_list_at(value, &["cards"], cards_total, "cards");
                        if ensured {
                            changed = true;
                            set_card_stats(value, cards_total, &stats_by_type);
                        }
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["cards"]);
                        recompute_card_stats(value, "cards");
                    }
                    if json_len_chars(value) > limit {
                        changed |= compact_stats_by_type(value);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_context", result)
        } else {
            ai_ok_with_warnings("think_context", result, warnings, Vec::new())
        }
    }
}
