#![forbid(unsafe_code)]
//! Thinking tools (cards, context, packs) (split-friendly module root).

mod add;
mod cards;
mod context;
mod frontier;
mod lane_context;
mod lint;
mod manage;
mod next;
mod pack;
mod pipeline;
mod query;
mod step_context;

use serde_json::Value;

pub(crate) use step_context::{ResolvedStepContext, resolve_step_context_from_args};

#[derive(Clone, Copy)]
struct ThinkFrontierLimits {
    hypotheses: usize,
    questions: usize,
    subgoals: usize,
    tests: usize,
}

struct ThinkFrontier {
    hypotheses: Vec<Value>,
    questions: Vec<Value>,
    subgoals: Vec<Value>,
    tests: Vec<Value>,
}
