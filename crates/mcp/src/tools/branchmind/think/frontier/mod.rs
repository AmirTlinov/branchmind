#![forbid(unsafe_code)]
//! Think frontier tools (frontier view + shared builder).

mod args;
mod budget;
mod build;
mod query;

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

        let ThinkFrontier {
            hypotheses,
            questions,
            subgoals,
            tests,
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
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

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
