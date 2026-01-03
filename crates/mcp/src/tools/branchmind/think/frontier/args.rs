#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct FrontierArgs {
    pub(super) workspace: WorkspaceId,
    pub(super) limit_hypotheses: usize,
    pub(super) limit_questions: usize,
    pub(super) limit_subgoals: usize,
    pub(super) limit_tests: usize,
    pub(super) max_chars: Option<usize>,
}

pub(super) fn parse(args_obj: &serde_json::Map<String, Value>) -> Result<FrontierArgs, Value> {
    let workspace = require_workspace(args_obj)?;
    let limit_hypotheses = optional_usize(args_obj, "limit_hypotheses")?.unwrap_or(5);
    let limit_questions = optional_usize(args_obj, "limit_questions")?.unwrap_or(5);
    let limit_subgoals = optional_usize(args_obj, "limit_subgoals")?.unwrap_or(5);
    let limit_tests = optional_usize(args_obj, "limit_tests")?.unwrap_or(5);
    let max_chars = optional_usize(args_obj, "max_chars")?;

    Ok(FrontierArgs {
        workspace,
        limit_hypotheses,
        limit_questions,
        limit_subgoals,
        limit_tests,
        max_chars,
    })
}
