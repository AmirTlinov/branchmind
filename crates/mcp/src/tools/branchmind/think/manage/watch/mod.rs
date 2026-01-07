#![forbid(unsafe_code)]

mod args;
mod budget;
mod candidates;
mod capsule;
mod trace;

use super::super::step_context::resolve_step_context_from_args;
use super::super::{ThinkFrontier, ThinkFrontierLimits};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_watch(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let args = match args::parse(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, graph_doc, trace_doc) =
            match self.resolve_think_watch_scope(&args.workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let step_ctx = if let Some(step_raw) = args.step.as_deref() {
            let ctx =
                match resolve_step_context_from_args(self, &args.workspace, args_obj, step_raw) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            Some(ctx)
        } else {
            None
        };
        let step_tag = step_ctx.as_ref().map(|ctx| ctx.step_tag.as_str());

        let ThinkFrontier {
            hypotheses: mut frontier_hypotheses,
            questions: mut frontier_questions,
            subgoals: mut frontier_subgoals,
            tests: mut frontier_tests,
        } = match self.build_think_frontier(
            &args.workspace,
            &branch,
            &graph_doc,
            ThinkFrontierLimits {
                hypotheses: args.limit_hypotheses,
                questions: args.limit_questions,
                subgoals: args.limit_subgoals,
                tests: args.limit_tests,
            },
            step_tag,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let candidates = match candidates::fetch(
            self,
            &args.workspace,
            &branch,
            &graph_doc,
            args.limit_candidates,
            args.warm_archive,
            step_tag,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // Multi-agent lanes (anti-noise): default reads include shared + "my lane" only.
        // Legacy cards without a lane tag are treated as shared.
        let mut candidates = candidates;
        if !args.all_lanes {
            let agent_id = args.agent_id.as_deref();
            candidates
                .cards
                .retain(|card| lane_matches_card_value(card, agent_id));

            let mut kept = std::collections::BTreeSet::<String>::new();
            for card in &candidates.cards {
                if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
                    kept.insert(id.to_string());
                }
            }
            candidates.edges.retain(|edge| {
                let from = edge.get("from").and_then(|v| v.as_str());
                let to = edge.get("to").and_then(|v| v.as_str());
                match (from, to) {
                    (Some(from), Some(to)) => kept.contains(from) && kept.contains(to),
                    _ => false,
                }
            });
        }

        let trace = match trace::fetch(
            self,
            &args.workspace,
            &branch,
            &trace_doc,
            args.trace_limit_steps,
            args.trace_statement_max_bytes,
            args.agent_id.as_deref(),
            args.all_lanes,
            step_ctx.as_ref().map(|ctx| ctx.task_id.as_str()),
            step_ctx.as_ref().map(|ctx| ctx.step.path.as_str()),
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let sequential = derive_trace_sequential_graph(&trace.entries);

        let engine = derive_reasoning_engine(
            EngineScope {
                workspace: args.workspace.as_str(),
                branch: branch.as_str(),
                graph_doc: graph_doc.as_str(),
                trace_doc: trace_doc.as_str(),
            },
            &candidates.cards,
            &candidates.edges,
            &trace.entries,
            EngineLimits {
                signals_limit: args.engine_signals_limit,
                actions_limit: args.engine_actions_limit,
            },
        );

        if !args.all_lanes {
            let agent_id = args.agent_id.as_deref();
            frontier_hypotheses.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_questions.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_subgoals.retain(|card| lane_matches_card_value(card, agent_id));
            frontier_tests.retain(|card| lane_matches_card_value(card, agent_id));
        }

        let candidates_total = candidates.cards.len();
        let frontier_counts = (
            frontier_hypotheses.len(),
            frontier_questions.len(),
            frontier_subgoals.len(),
            frontier_tests.len(),
        );

        let lane_summary = if args.all_lanes {
            let mut cards = Vec::<Value>::new();
            cards.extend(candidates.cards.iter().cloned());
            cards.extend(frontier_hypotheses.iter().cloned());
            cards.extend(frontier_questions.iter().cloned());
            cards.extend(frontier_subgoals.iter().cloned());
            cards.extend(frontier_tests.iter().cloned());
            Some(build_lane_summary(&cards, 8))
        } else {
            None
        };

        let capsule = capsule::build_watch_capsule(capsule::WatchCapsuleArgs {
            workspace: &args.workspace,
            branch: branch.as_str(),
            graph_doc: graph_doc.as_str(),
            trace_doc: trace_doc.as_str(),
            agent_id: args.agent_id.as_deref(),
            all_lanes: args.all_lanes,
            step_ctx: step_ctx.as_ref(),
            engine: engine.as_ref(),
        });

        let mut result = json!({
            "workspace": args.workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "trace_doc": trace_doc,
            "capsule": capsule,
            "candidates": candidates.cards,
            "frontier": {
                "hypotheses": frontier_hypotheses,
                "questions": frontier_questions,
                "subgoals": frontier_subgoals,
                "tests": frontier_tests
            },
            "trace": {
                "entries": trace.entries,
                "sequential": sequential.unwrap_or(Value::Null),
                "pagination": {
                    "cursor": Value::Null,
                    "next_cursor": trace.next_cursor,
                    "has_more": trace.has_more,
                    "limit": args.trace_limit_steps,
                    "count": trace.count
                }
            },
            "truncated": false
        });
        if let Some(obj) = result.as_object_mut() {
            if let Some(engine) = engine {
                obj.insert("engine".to_string(), engine);
            }
            if let Some(lane_summary) = lane_summary {
                obj.insert("lane_summary".to_string(), lane_summary);
            }
        }

        let warnings = match args.max_chars {
            None => Vec::new(),
            Some(max_chars) => budget::enforce(
                &mut result,
                max_chars,
                budget::WatchTotals {
                    candidates_total,
                    trace_total: trace.count,
                    frontier_hypotheses_total: frontier_counts.0,
                    frontier_questions_total: frontier_counts.1,
                    frontier_subgoals_total: frontier_counts.2,
                    frontier_tests_total: frontier_counts.3,
                },
            ),
        };

        let entries_snapshot = result
            .get("trace")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(sequential) = result
            .get_mut("trace")
            .and_then(|v| v.get_mut("sequential"))
        {
            filter_trace_sequential_graph_to_entries(sequential, &entries_snapshot);
        }

        let candidates_snapshot = result
            .get("candidates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(engine) = result.get_mut("engine") {
            filter_engine_to_cards(engine, &candidates_snapshot);
        }
        if let Some(capsule) = result.get_mut("capsule") {
            capsule::filter_watch_capsule_to_cards(capsule, &candidates_snapshot);
        }

        if warnings.is_empty() {
            ai_ok("think_watch", result)
        } else {
            ai_ok_with_warnings("think_watch", result, warnings, Vec::new())
        }
    }
}
