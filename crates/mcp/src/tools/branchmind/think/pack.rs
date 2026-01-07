#![forbid(unsafe_code)]

use super::step_context::resolve_step_context_from_args;
use super::{ThinkFrontier, ThinkFrontierLimits};
#[path = "pack_capsule.rs"]
mod pack_capsule;
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
        let warm_archive = view.warm_archive();
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let all_lanes = args_obj
            .get("all_lanes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let all_lanes = all_lanes || view.implies_all_lanes();
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
        let step = match optional_string(args_obj, "step") {
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

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let reference = match optional_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let graph_doc_override = match optional_string(args_obj, "graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Err(resp) = ensure_nonempty_doc(&graph_doc_override, "graph_doc") {
            return resp;
        }

        let scope = match self.resolve_reasoning_scope(
            &workspace,
            ReasoningScopeInput {
                target,
                branch: reference,
                notes_doc: None,
                graph_doc: graph_doc_override,
                trace_doc: None,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let branch = scope.branch;
        let graph_doc = scope.graph_doc;
        let trace_doc = scope.trace_doc;

        let step_ctx = if let Some(step_raw) = step {
            let ctx = match resolve_step_context_from_args(self, &workspace, args_obj, &step_raw) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            Some(ctx)
        } else {
            None
        };
        let focus_step_tag = step_ctx.as_ref().map(|ctx| ctx.step_tag.as_str());
        let lane_multiplier = if all_lanes {
            1usize
        } else if agent_id.is_some() {
            2usize
        } else {
            1usize
        };
        let candidates = match fetch_relevance_first_cards(
            self,
            &workspace,
            &branch,
            &graph_doc,
            limit_candidates,
            focus_step_tag,
            agent_id.as_deref(),
            warm_archive,
            all_lanes,
            false,
        ) {
            Ok(v) => v.cards,
            Err(resp) => return resp,
        };

        let mut by_type = std::collections::BTreeMap::<String, u64>::new();
        for card in &candidates {
            if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                *by_type.entry(ty.to_string()).or_insert(0) += 1;
            }
        }
        let candidate_count = candidates.len();

        let ThinkFrontier {
            hypotheses: mut frontier_hypotheses,
            questions: mut frontier_questions,
            subgoals: mut frontier_subgoals,
            tests: mut frontier_tests,
        } = match self.build_think_frontier(
            &workspace,
            &branch,
            &graph_doc,
            ThinkFrontierLimits {
                hypotheses: limit_hypotheses.saturating_mul(lane_multiplier),
                questions: limit_questions.saturating_mul(lane_multiplier),
                subgoals: limit_subgoals.saturating_mul(lane_multiplier),
                tests: limit_tests.saturating_mul(lane_multiplier),
            },
            focus_step_tag,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = agent_id.as_deref();
        if !all_lanes {
            frontier_hypotheses.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_questions.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_subgoals.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_tests.retain(|card| lane_matches_card_value(card, agent_id));
        }
        frontier_hypotheses.truncate(limit_hypotheses);
        frontier_questions.truncate(limit_questions);
        frontier_subgoals.truncate(limit_subgoals);
        frontier_tests.truncate(limit_tests);
        let frontier_counts = (
            frontier_hypotheses.len(),
            frontier_questions.len(),
            frontier_subgoals.len(),
            frontier_tests.len(),
        );

        // Build a small connected edges slice for the returned cards to power the engine.
        let mut engine_cards = Vec::<Value>::new();
        engine_cards.extend(candidates.iter().cloned());
        engine_cards.extend(frontier_hypotheses.iter().cloned());
        engine_cards.extend(frontier_questions.iter().cloned());
        engine_cards.extend(frontier_subgoals.iter().cloned());
        engine_cards.extend(frontier_tests.iter().cloned());
        let mut engine_ids = Vec::<String>::new();
        {
            let mut seen = std::collections::BTreeSet::<String>::new();
            for card in &engine_cards {
                let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                    continue;
                };
                if seen.insert(id.to_string()) {
                    engine_ids.push(id.to_string());
                }
            }
        }
        let edges = if engine_ids.is_empty() {
            Vec::new()
        } else {
            match self.store.graph_query(
                &workspace,
                &branch,
                &graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: Some(engine_ids.clone()),
                    types: None,
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text: None,
                    cursor: None,
                    limit: engine_ids.len().max(1),
                    include_edges: true,
                    edges_limit: (engine_ids.len().saturating_mul(6)).min(200),
                },
            ) {
                Ok(v) => graph_edges_to_json(v.edges),
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
            }
        };

        let engine = derive_reasoning_engine_step_aware(
            EngineScope {
                workspace: workspace.as_str(),
                branch: branch.as_str(),
                graph_doc: graph_doc.as_str(),
                trace_doc: trace_doc.as_str(),
            },
            &engine_cards,
            &edges,
            &[],
            focus_step_tag,
            EngineLimits {
                signals_limit: 4,
                actions_limit: 2,
            },
        );

        let lane_summary = if all_lanes {
            let mut cards = Vec::<Value>::new();
            cards.extend(candidates.iter().cloned());
            cards.extend(frontier_hypotheses.iter().cloned());
            cards.extend(frontier_questions.iter().cloned());
            cards.extend(frontier_subgoals.iter().cloned());
            cards.extend(frontier_tests.iter().cloned());
            Some(build_lane_summary(&cards, 8))
        } else {
            None
        };

        let capsule = pack_capsule::build_think_pack_capsule(pack_capsule::ThinkPackCapsuleArgs {
            workspace: &workspace,
            branch: branch.as_str(),
            graph_doc: graph_doc.as_str(),
            trace_doc: trace_doc.as_str(),
            agent_id: agent_id.as_deref(),
            all_lanes,
            step_ctx: step_ctx.as_ref(),
            engine: engine.as_ref(),
        });

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "trace_doc": trace_doc,
            "capsule": capsule,
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
        if let Some(lane_summary) = lane_summary
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("lane_summary".to_string(), lane_summary);
        }
        if let Some(obj) = result.as_object_mut()
            && let Some(engine) = engine
        {
            obj.insert("engine".to_string(), engine);
        }

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
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &[], &["lane_summary"]);
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &["engine"], &["actions"]);
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &["engine"], &["signals"]);
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &[], &["engine"]);
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &["capsule", "why"], &["signals"]);
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(&mut result, &["capsule", "next"], &["backup"]);
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
                        changed |= drop_fields_at(value, &[], &["lane_summary"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["engine"], &["actions"]);
                        changed |= drop_fields_at(value, &["engine"], &["signals"]);
                        changed |= drop_fields_at(value, &[], &["engine"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["capsule", "why"], &["signals"]);
                        changed |= drop_fields_at(value, &["capsule", "next"], &["backup"]);
                    }
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
                    if json_len_chars(value) > limit {
                        let Some(obj) = value.as_object_mut() else {
                            return changed;
                        };
                        let capsule = obj.remove("capsule");
                        obj.clear();
                        if let Some(capsule) = capsule {
                            obj.insert("capsule".to_string(), capsule);
                        }
                        obj.insert("truncated".to_string(), Value::Bool(true));
                        changed = true;
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        // Post-budget hygiene: keep engine/capsule actions referencing only returned cards.
        let mut cards_snapshot = Vec::<Value>::new();
        if let Some(candidates) = result.get("candidates").and_then(|v| v.as_array()) {
            cards_snapshot.extend(candidates.iter().cloned());
        }
        if let Some(frontier) = result.get("frontier").and_then(|v| v.as_object()) {
            for key in ["hypotheses", "questions", "subgoals", "tests"] {
                if let Some(arr) = frontier.get(key).and_then(|v| v.as_array()) {
                    cards_snapshot.extend(arr.iter().cloned());
                }
            }
        }
        if let Some(engine) = result.get_mut("engine") {
            filter_engine_to_cards(engine, &cards_snapshot);
        }
        if let Some(capsule) = result.get_mut("capsule") {
            pack_capsule::filter_think_pack_capsule_to_cards(capsule, &cards_snapshot);
        }

        if warnings.is_empty() {
            ai_ok("think_pack", result)
        } else {
            ai_ok_with_warnings("think_pack", result, warnings, Vec::new())
        }
    }
}
