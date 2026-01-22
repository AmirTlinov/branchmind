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

        let context_budget = match optional_usize(args_obj, "context_budget") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let view = match parse_relevance_view(
            args_obj,
            "view",
            if context_budget.is_some() {
                RelevanceView::Smart
            } else {
                RelevanceView::Explore
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let all_lanes = match optional_bool(args_obj, "all_lanes") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let warm_archive = view.warm_archive();
        let all_lanes = all_lanes || view.implies_all_lanes();
        let step = match optional_string(args_obj, "step") {
            Ok(v) => v,
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

        let step_ctx = if let Some(step_raw) = step.as_deref() {
            match super::step_context::resolve_step_context_from_args(
                self, &workspace, args_obj, step_raw,
            ) {
                Ok(v) => Some(v),
                Err(resp) => return resp,
            }
        } else {
            None
        };
        let step_tag = step_ctx.as_ref().map(|ctx| ctx.step_tag.as_str());

        let limit_cards = match optional_usize(args_obj, "limit_cards") {
            Ok(v) => v.unwrap_or(30),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match (context_budget, max_chars) {
            (None, v) => v,
            (Some(budget), None) => Some(budget),
            (Some(budget), Some(explicit)) => Some(explicit.min(budget)),
        };

        let cards = match fetch_relevance_first_cards(
            self,
            &workspace,
            RelevanceFirstCardsRequest {
                branch: &branch,
                graph_doc: &graph_doc,
                cards_limit: limit_cards,
                focus_step_tag: step_tag,
                agent_id: agent_id.as_deref(),
                warm_archive,
                all_lanes,
                read_only: false,
            },
        ) {
            Ok(v) => v.cards,
            Err(err) => return err,
        };

        let mut by_type = std::collections::BTreeMap::<String, u64>::new();
        for card in &cards {
            if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                *by_type.entry(ty.to_string()).or_insert(0) += 1;
            }
        }

        let cards_total = cards.len();
        let stats_by_type = by_type.clone();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "step_focus": step_ctx.as_ref().map(|ctx| json!({
                "task_id": ctx.task_id,
                "step_id": ctx.step.step_id,
                "path": ctx.step.path,
                "tag": ctx.step_tag
            })).unwrap_or(Value::Null),
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
