#![forbid(unsafe_code)]

use super::step_context::resolve_step_context_from_args;
use super::{ThinkFrontier, ThinkFrontierLimits};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_next(&mut self, args: Value) -> Value {
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
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match (context_budget, max_chars) {
            (None, v) => v,
            (Some(budget), None) => Some(budget),
            (Some(budget), Some(explicit)) => Some(explicit.min(budget)),
        };
        let step = match optional_string(args_obj, "step") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let _agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let include_drafts = match optional_bool(args_obj, "include_drafts") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let all_lanes = match optional_bool(args_obj, "all_lanes") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let all_lanes = all_lanes || include_drafts || view.implies_all_lanes();
        let (branch, graph_doc) = match self.resolve_think_graph_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let step_tag = if let Some(step_raw) = step {
            let ctx = match resolve_step_context_from_args(self, &workspace, args_obj, &step_raw) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            ctx.map(|ctx| ctx.step_tag)
        } else {
            None
        };

        let ThinkFrontier {
            mut hypotheses,
            mut questions,
            mut subgoals,
            mut tests,
        } = match self.build_think_frontier(
            &workspace,
            &branch,
            &graph_doc,
            ThinkFrontierLimits {
                hypotheses: 5,
                questions: 5,
                subgoals: 5,
                tests: 5,
            },
            step_tag.as_deref(),
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if !all_lanes {
            hypotheses
                .retain(|card| card_value_visibility_allows(card, false, step_tag.as_deref()));
            questions.retain(|card| card_value_visibility_allows(card, false, step_tag.as_deref()));
            subgoals.retain(|card| card_value_visibility_allows(card, false, step_tag.as_deref()));
            tests.retain(|card| card_value_visibility_allows(card, false, step_tag.as_deref()));
        }

        let mut best: Option<Value> = None;
        let mut best_seq: i64 = -1;
        for list in [&questions, &hypotheses, &subgoals, &tests] {
            for item in list {
                let seq = item.get("last_seq").and_then(|v| v.as_i64()).unwrap_or(-1);
                if seq > best_seq {
                    best_seq = seq;
                    best = Some(item.clone());
                }
            }
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "candidate": best,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;
            let mut forced_minimal = false;

            let candidate_stub = result.get("candidate").cloned().map(|mut value| {
                minimalize_card_value(&mut value);
                value
            });

            if json_len_chars(&result) > limit {
                let compacted = result
                    .get_mut("candidate")
                    .map(|candidate| compact_card_value(candidate, 160, true, true, false))
                    .unwrap_or(false);
                truncated |= compacted;
            }
            if json_len_chars(&result) > limit {
                let minimized = result
                    .get_mut("candidate")
                    .map(minimalize_card_value)
                    .unwrap_or(false);
                truncated |= minimized;
                minimal |= minimized;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= value
                            .get_mut("candidate")
                            .map(minimalize_card_value)
                            .unwrap_or(false);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["graph_doc"]);
                    }
                    if json_len_chars(value) > limit {
                        *value = minimal_next_value(limit, candidate_stub.clone());
                        forced_minimal = true;
                        changed = true;
                    }
                    changed
                });

            if forced_minimal {
                truncated = true;
                minimal = true;
            }
            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_next", result)
        } else {
            ai_ok_with_warnings("think_next", result, warnings, Vec::new())
        }
    }
}
