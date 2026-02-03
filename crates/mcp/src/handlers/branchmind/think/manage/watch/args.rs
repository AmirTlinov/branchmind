#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct WatchArgs {
    pub(super) workspace: WorkspaceId,
    pub(super) step: Option<String>,
    pub(super) agent_id: Option<String>,
    pub(super) all_lanes: bool,
    pub(super) warm_archive: bool,
    pub(super) limit_candidates: usize,
    pub(super) limit_hypotheses: usize,
    pub(super) limit_questions: usize,
    pub(super) limit_subgoals: usize,
    pub(super) limit_tests: usize,
    pub(super) trace_limit_steps: usize,
    pub(super) trace_statement_max_bytes: Option<usize>,
    pub(super) engine_signals_limit: usize,
    pub(super) engine_actions_limit: usize,
    pub(super) max_chars: Option<usize>,
}

pub(super) fn parse(args_obj: &serde_json::Map<String, Value>) -> Result<WatchArgs, Value> {
    let workspace = require_workspace(args_obj)?;

    let context_budget = optional_usize(args_obj, "context_budget")?;
    let view = parse_relevance_view(
        args_obj,
        "view",
        if context_budget.is_some() {
            RelevanceView::Smart
        } else {
            RelevanceView::Explore
        },
    )?;
    let warm_archive = view.warm_archive();
    let step = optional_string(args_obj, "step")?;
    let agent_id = optional_agent_id(args_obj, "agent_id")?;
    let include_drafts = optional_bool(args_obj, "include_drafts")?.unwrap_or(false);
    let all_lanes = optional_bool(args_obj, "all_lanes")?.unwrap_or(false);
    let all_lanes = all_lanes || include_drafts || view.implies_all_lanes();
    let limit_candidates = optional_usize(args_obj, "limit_candidates")?.unwrap_or(30);
    let limit_hypotheses = optional_usize(args_obj, "limit_hypotheses")?.unwrap_or(5);
    let limit_questions = optional_usize(args_obj, "limit_questions")?.unwrap_or(5);
    let limit_subgoals = optional_usize(args_obj, "limit_subgoals")?.unwrap_or(5);
    let limit_tests = optional_usize(args_obj, "limit_tests")?.unwrap_or(5);
    let trace_limit_steps = optional_usize(args_obj, "trace_limit_steps")?.unwrap_or(20);
    let trace_statement_max_bytes = optional_usize(args_obj, "trace_statement_max_bytes")?;
    let engine_signals_limit = optional_usize(args_obj, "engine_signals_limit")?.unwrap_or(6);
    // Default to one best action + one backup to keep the watch surface focused.
    let engine_actions_limit = optional_usize(args_obj, "engine_actions_limit")?.unwrap_or(2);
    let max_chars = optional_usize(args_obj, "max_chars")?;
    let max_chars = match (context_budget, max_chars) {
        (None, v) => v,
        (Some(budget), None) => Some(budget),
        (Some(budget), Some(explicit)) => Some(explicit.min(budget)),
    };

    Ok(WatchArgs {
        workspace,
        step,
        agent_id,
        all_lanes,
        warm_archive,
        limit_candidates,
        limit_hypotheses,
        limit_questions,
        limit_subgoals,
        limit_tests,
        trace_limit_steps,
        trace_statement_max_bytes,
        engine_signals_limit,
        engine_actions_limit,
        max_chars,
    })
}
