#![forbid(unsafe_code)]
//! Thinking tools (cards, context, packs) (split-friendly module root).

mod add;
mod cards;
mod context;
mod frontier;
mod lint;
mod manage;
mod next;
mod pack;
mod pipeline;
mod query;

use serde_json::Value;

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
