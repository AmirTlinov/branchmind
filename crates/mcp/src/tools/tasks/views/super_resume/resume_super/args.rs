#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperArgs {
    pub(super) workspace: WorkspaceId,
    pub(super) max_chars: Option<usize>,
    pub(super) events_limit: usize,
    pub(super) decisions_limit: usize,
    pub(super) evidence_limit: usize,
    pub(super) blockers_limit: usize,
    pub(super) notes_limit: usize,
    pub(super) trace_limit: usize,
    pub(super) cards_limit: usize,
    pub(super) notes_cursor: Option<i64>,
    pub(super) trace_cursor: Option<i64>,
    pub(super) cards_cursor: Option<i64>,
    pub(super) graph_diff_cursor: Option<i64>,
    pub(super) graph_diff_limit: usize,
    pub(super) include_graph_diff: bool,
    pub(super) read_only: bool,
    pub(super) explicit_target: Option<String>,
}

pub(super) fn parse_resume_super_args(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<ResumeSuperArgs, Value> {
    let workspace = require_workspace(args_obj)?;
    let max_chars = optional_usize(args_obj, "max_chars")?;

    let events_limit = args_obj
        .get("events_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(20);
    let decisions_limit = args_obj
        .get("decisions_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(5);
    let evidence_limit = args_obj
        .get("evidence_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(5);
    let blockers_limit = args_obj
        .get("blockers_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(5);
    let notes_limit = args_obj
        .get("notes_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(10);
    let trace_limit = args_obj
        .get("trace_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(20);
    let cards_limit = args_obj
        .get("cards_limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(20);

    let notes_cursor = optional_i64(args_obj, "notes_cursor")?;
    let trace_cursor = optional_i64(args_obj, "trace_cursor")?;
    let cards_cursor = optional_i64(args_obj, "cards_cursor")?;

    let graph_diff_cursor = optional_i64(args_obj, "graph_diff_cursor")?;
    let graph_diff_limit = optional_usize(args_obj, "graph_diff_limit")?;
    let graph_diff = args_obj
        .get("graph_diff")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let include_graph_diff =
        graph_diff || graph_diff_limit.is_some() || graph_diff_cursor.is_some();
    let graph_diff_limit = graph_diff_limit.unwrap_or(50).max(1);

    let read_only = args_obj
        .get("read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let explicit_target = args_obj
        .get("task")
        .and_then(|v| v.as_str())
        .or_else(|| args_obj.get("plan").and_then(|v| v.as_str()))
        .map(|v| v.to_string());

    Ok(ResumeSuperArgs {
        workspace,
        max_chars,
        events_limit,
        decisions_limit,
        evidence_limit,
        blockers_limit,
        notes_limit,
        trace_limit,
        cards_limit,
        notes_cursor,
        trace_cursor,
        cards_cursor,
        graph_diff_cursor,
        graph_diff_limit,
        include_graph_diff,
        read_only,
        explicit_target,
    })
}
