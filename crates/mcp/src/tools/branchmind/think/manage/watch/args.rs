#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct WatchArgs {
    pub(super) workspace: WorkspaceId,
    pub(super) limit_candidates: usize,
    pub(super) limit_hypotheses: usize,
    pub(super) limit_questions: usize,
    pub(super) limit_subgoals: usize,
    pub(super) limit_tests: usize,
    pub(super) trace_limit_steps: usize,
    pub(super) trace_statement_max_bytes: Option<usize>,
    pub(super) max_chars: Option<usize>,
}

pub(super) fn parse(args_obj: &serde_json::Map<String, Value>) -> Result<WatchArgs, Value> {
    let workspace = require_workspace(args_obj)?;

    let limit_candidates = optional_usize(args_obj, "limit_candidates")?.unwrap_or(30);
    let limit_hypotheses = optional_usize(args_obj, "limit_hypotheses")?.unwrap_or(5);
    let limit_questions = optional_usize(args_obj, "limit_questions")?.unwrap_or(5);
    let limit_subgoals = optional_usize(args_obj, "limit_subgoals")?.unwrap_or(5);
    let limit_tests = optional_usize(args_obj, "limit_tests")?.unwrap_or(5);
    let trace_limit_steps = optional_usize(args_obj, "trace_limit_steps")?.unwrap_or(20);
    let trace_statement_max_bytes = optional_usize(args_obj, "trace_statement_max_bytes")?;
    let max_chars = optional_usize(args_obj, "max_chars")?;

    Ok(WatchArgs {
        workspace,
        limit_candidates,
        limit_hypotheses,
        limit_questions,
        limit_subgoals,
        limit_tests,
        trace_limit_steps,
        trace_statement_max_bytes,
        max_chars,
    })
}
