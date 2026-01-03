#![forbid(unsafe_code)]

mod args;
mod budget;
mod candidates;
mod trace;

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

        let ThinkFrontier {
            hypotheses: frontier_hypotheses,
            questions: frontier_questions,
            subgoals: frontier_subgoals,
            tests: frontier_tests,
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

        let candidates = match candidates::fetch(
            self,
            &args.workspace,
            &branch,
            &graph_doc,
            args.limit_candidates,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let trace = match trace::fetch(
            self,
            &args.workspace,
            &branch,
            &trace_doc,
            args.trace_limit_steps,
            args.trace_statement_max_bytes,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let candidates_total = candidates.len();
        let frontier_counts = (
            frontier_hypotheses.len(),
            frontier_questions.len(),
            frontier_subgoals.len(),
            frontier_tests.len(),
        );

        let mut result = json!({
            "workspace": args.workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "trace_doc": trace_doc,
            "candidates": candidates,
            "frontier": {
                "hypotheses": frontier_hypotheses,
                "questions": frontier_questions,
                "subgoals": frontier_subgoals,
                "tests": frontier_tests
            },
            "trace": {
                "entries": trace.entries,
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

        if warnings.is_empty() {
            ai_ok("think_watch", result)
        } else {
            ai_ok_with_warnings("think_watch", result, warnings, Vec::new())
        }
    }
}
