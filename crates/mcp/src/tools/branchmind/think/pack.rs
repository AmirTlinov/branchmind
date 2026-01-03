#![forbid(unsafe_code)]

use super::{ThinkFrontier, ThinkFrontierLimits};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_pack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit_candidates = match optional_usize(args_obj, "limit_candidates") {
            Ok(v) => v.unwrap_or(30),
            Err(resp) => return resp,
        };
        let limit_hypotheses = match optional_usize(args_obj, "limit_hypotheses") {
            Ok(v) => v.unwrap_or(5),
            Err(resp) => return resp,
        };
        let limit_questions = match optional_usize(args_obj, "limit_questions") {
            Ok(v) => v.unwrap_or(5),
            Err(resp) => return resp,
        };
        let limit_subgoals = match optional_usize(args_obj, "limit_subgoals") {
            Ok(v) => v.unwrap_or(5),
            Err(resp) => return resp,
        };
        let limit_tests = match optional_usize(args_obj, "limit_tests") {
            Ok(v) => v.unwrap_or(5),
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

        let candidates = graph_nodes_to_cards(slice.nodes);
        let mut by_type = std::collections::BTreeMap::<String, u64>::new();
        for card in &candidates {
            if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                *by_type.entry(ty.to_string()).or_insert(0) += 1;
            }
        }
        let candidate_count = candidates.len();

        let ThinkFrontier {
            hypotheses: frontier_hypotheses,
            questions: frontier_questions,
            subgoals: frontier_subgoals,
            tests: frontier_tests,
        } = match self.build_think_frontier(
            &workspace,
            &branch,
            &graph_doc,
            ThinkFrontierLimits {
                hypotheses: limit_hypotheses,
                questions: limit_questions,
                subgoals: limit_subgoals,
                tests: limit_tests,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let frontier_counts = (
            frontier_hypotheses.len(),
            frontier_questions.len(),
            frontier_subgoals.len(),
            frontier_tests.len(),
        );

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "stats": { "cards": candidate_count, "by_type": by_type },
            "candidates": candidates,
            "frontier": {
                "hypotheses": frontier_hypotheses,
                "questions": frontier_questions,
                "subgoals": frontier_subgoals,
                "tests": frontier_tests
            },
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |=
                    compact_card_fields_at(&mut result, &["candidates"], 180, true, true, false);
                for path in [
                    &["frontier", "tests"][..],
                    &["frontier", "subgoals"][..],
                    &["frontier", "questions"][..],
                    &["frontier", "hypotheses"][..],
                ] {
                    truncated |= compact_card_fields_at(&mut result, path, 180, true, true, false);
                }
            }

            let candidates_trimmed =
                trim_array_to_budget(&mut result, &["candidates"], limit, false);
            truncated |= candidates_trimmed;
            if candidates_trimmed {
                if ensure_minimal_list_at(
                    &mut result,
                    &["candidates"],
                    candidate_count,
                    "candidates",
                ) {
                    minimal = true;
                    set_card_stats(&mut result, candidate_count, &by_type);
                } else {
                    recompute_card_stats(&mut result, "candidates");
                }
            }
            if json_len_chars(&result) > limit {
                for path in [
                    &["frontier", "tests"][..],
                    &["frontier", "subgoals"][..],
                    &["frontier", "questions"][..],
                    &["frontier", "hypotheses"][..],
                ] {
                    if json_len_chars(&result) <= limit {
                        break;
                    }
                    truncated |= trim_array_to_budget(&mut result, path, limit, false);
                }
            }
            let (hypotheses_total, questions_total, subgoals_total, tests_total) = frontier_counts;
            let frontier_specs = [
                (
                    &["frontier", "hypotheses"][..],
                    hypotheses_total,
                    "hypotheses",
                ),
                (&["frontier", "questions"][..], questions_total, "questions"),
                (&["frontier", "subgoals"][..], subgoals_total, "subgoals"),
                (&["frontier", "tests"][..], tests_total, "tests"),
            ];
            for (path, total, label) in frontier_specs {
                let empty = result
                    .get(path[0])
                    .and_then(|v| v.get(path[1]))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.is_empty())
                    .unwrap_or(true);
                if empty && total > 0 && ensure_minimal_list_at(&mut result, path, total, label) {
                    truncated = true;
                    minimal = true;
                }
            }
            if json_len_chars(&result) > limit && compact_stats_by_type(&mut result) {
                truncated = true;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |=
                            compact_card_fields_at(value, &["candidates"], 120, true, true, true);
                        for path in [
                            &["frontier", "tests"][..],
                            &["frontier", "subgoals"][..],
                            &["frontier", "questions"][..],
                            &["frontier", "hypotheses"][..],
                        ] {
                            changed |= compact_card_fields_at(value, path, 120, true, true, true);
                        }
                    }
                    if json_len_chars(value) > limit {
                        if minimalize_cards_at(value, &["candidates"]) {
                            changed = true;
                        }
                        for path in [
                            &["frontier", "tests"][..],
                            &["frontier", "subgoals"][..],
                            &["frontier", "questions"][..],
                            &["frontier", "hypotheses"][..],
                        ] {
                            if minimalize_cards_at(value, path) {
                                changed = true;
                            }
                        }
                    }
                    if json_len_chars(value) > limit {
                        let retained = retain_one_at(value, &["candidates"], true);
                        if retained {
                            changed = true;
                            recompute_card_stats(value, "candidates");
                        }
                    }
                    if json_len_chars(value) > limit {
                        if ensure_minimal_list_at(
                            value,
                            &["candidates"],
                            candidate_count,
                            "candidates",
                        ) {
                            changed = true;
                            set_card_stats(value, candidate_count, &by_type);
                        }
                        for (path, total, label) in [
                            (
                                &["frontier", "hypotheses"][..],
                                hypotheses_total,
                                "hypotheses",
                            ),
                            (&["frontier", "questions"][..], questions_total, "questions"),
                            (&["frontier", "subgoals"][..], subgoals_total, "subgoals"),
                            (&["frontier", "tests"][..], tests_total, "tests"),
                        ] {
                            if ensure_minimal_list_at(value, path, total, label) {
                                changed = true;
                            }
                        }
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(
                            value,
                            &["frontier"],
                            &["tests", "subgoals", "questions", "hypotheses"],
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["candidates"]);
                        recompute_card_stats(value, "candidates");
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
            ai_ok("think_pack", result)
        } else {
            ai_ok_with_warnings("think_pack", result, warnings, Vec::new())
        }
    }
}
