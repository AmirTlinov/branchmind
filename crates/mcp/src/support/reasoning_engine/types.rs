#![forbid(unsafe_code)]

use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EngineLimits {
    pub(crate) signals_limit: usize,
    pub(crate) actions_limit: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EngineScope<'a> {
    pub(crate) workspace: &'a str,
    pub(crate) branch: &'a str,
    pub(crate) graph_doc: &'a str,
    pub(crate) trace_doc: &'a str,
}

#[derive(Clone, Debug)]
pub(super) struct EngineSignal {
    pub(super) severity_rank: u8,
    pub(super) sort_ts_ms: i64,
    pub(super) code: &'static str,
    pub(super) severity: &'static str,
    pub(super) message: String,
    pub(super) refs: Vec<EngineRef>,
}

#[derive(Clone, Debug)]
pub(super) struct EngineAction {
    pub(super) priority_rank: u8,
    pub(super) sort_ts_ms: i64,
    pub(super) kind: &'static str,
    pub(super) priority: &'static str,
    pub(super) title: String,
    pub(super) why: Option<String>,
    pub(super) refs: Vec<EngineRef>,
    pub(super) calls: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct EngineRef {
    pub(super) kind: &'static str,
    pub(super) id: String,
}

pub(super) fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 4,
        "high" => 3,
        "warning" => 2,
        "info" => 1,
        _ => 0,
    }
}

pub(super) fn priority_rank(priority: &str) -> u8 {
    match priority {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}
