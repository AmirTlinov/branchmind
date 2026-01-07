#![forbid(unsafe_code)]
//! Think frontier tools (frontier view + shared builder).

mod args;
mod budget;
mod build;
mod query;

use super::step_context::resolve_step_context_from_args;
use super::{ThinkFrontier, ThinkFrontierLimits};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_frontier(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let args = match args::parse(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, graph_doc) = match self.resolve_think_graph_scope(&args.workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let step_tag = if let Some(step_raw) = args.step.as_deref() {
            let ctx =
                match resolve_step_context_from_args(self, &args.workspace, args_obj, step_raw) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            Some(ctx.step_tag)
        } else {
            None
        };

        let lane_multiplier = if args.all_lanes {
            1usize
        } else if args.agent_id.is_some() {
            2usize
        } else {
            1usize
        };

        let ThinkFrontier {
            mut hypotheses,
            mut questions,
            mut subgoals,
            mut tests,
        } = match self.build_think_frontier(
            &args.workspace,
            &branch,
            &graph_doc,
            ThinkFrontierLimits {
                hypotheses: args.limit_hypotheses.saturating_mul(lane_multiplier),
                questions: args.limit_questions.saturating_mul(lane_multiplier),
                subgoals: args.limit_subgoals.saturating_mul(lane_multiplier),
                tests: args.limit_tests.saturating_mul(lane_multiplier),
            },
            step_tag.as_deref(),
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let agent_id = args.agent_id.as_deref();
        if !args.all_lanes {
            hypotheses.retain(|card| lane_matches_card_value(card, agent_id));
            questions.retain(|card| lane_matches_card_value(card, agent_id));
            subgoals.retain(|card| lane_matches_card_value(card, agent_id));
            tests.retain(|card| lane_matches_card_value(card, agent_id));
        }
        hypotheses.truncate(args.limit_hypotheses);
        questions.truncate(args.limit_questions);
        subgoals.truncate(args.limit_subgoals);
        tests.truncate(args.limit_tests);

        let totals = budget::FrontierTotals {
            hypotheses_total: hypotheses.len(),
            questions_total: questions.len(),
            subgoals_total: subgoals.len(),
            tests_total: tests.len(),
        };

        let mut result = json!({
            "workspace": args.workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "frontier": {
                "hypotheses": hypotheses,
                "questions": questions,
                "subgoals": subgoals,
                "tests": tests
            },
            "truncated": false
        });

        let warnings = match args.max_chars {
            None => Vec::new(),
            Some(max_chars) => budget::enforce(&mut result, max_chars, totals),
        };

        if warnings.is_empty() {
            ai_ok("think_frontier", result)
        } else {
            ai_ok_with_warnings("think_frontier", result, warnings, Vec::new())
        }
    }
}
