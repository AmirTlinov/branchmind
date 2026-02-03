#![forbid(unsafe_code)]

use crate::*;
use bm_core::ids::WorkspaceId;
use serde_json::Value;

pub(super) struct ContextPackArgs {
    pub workspace: WorkspaceId,
    pub warm_archive: bool,
    pub requested_target: Option<String>,
    pub requested_ref: Option<String>,
    pub notes_doc: Option<String>,
    pub trace_doc: Option<String>,
    pub graph_doc: Option<String>,
    pub step: Option<String>,
    pub agent_id: Option<String>,
    pub all_lanes: bool,
    pub notes_limit: usize,
    pub trace_limit: usize,
    pub limit_cards: usize,
    pub decisions_limit: usize,
    pub evidence_limit: usize,
    pub blockers_limit: usize,
    pub max_chars: Option<usize>,
    pub read_only: bool,
}

pub(super) fn parse_context_pack_args(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<ContextPackArgs, Value> {
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

    let requested_target = args_obj
        .get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let requested_ref = optional_string(args_obj, "ref")?;
    let notes_doc = optional_string(args_obj, "notes_doc")?;
    let trace_doc = optional_string(args_obj, "trace_doc")?;
    let graph_doc = optional_string(args_obj, "graph_doc")?;
    let step = optional_string(args_obj, "step")?;
    let agent_id = optional_agent_id(args_obj, "agent_id")?;
    let include_drafts = optional_bool(args_obj, "include_drafts")?.unwrap_or(false);
    let all_lanes = optional_bool(args_obj, "all_lanes")?.unwrap_or(false);
    let all_lanes = all_lanes || include_drafts || view.implies_all_lanes();

    ensure_nonempty_doc(&notes_doc, "notes_doc")?;
    ensure_nonempty_doc(&trace_doc, "trace_doc")?;
    ensure_nonempty_doc(&graph_doc, "graph_doc")?;

    let notes_limit = optional_usize(args_obj, "notes_limit")?.unwrap_or(20);
    let trace_limit = optional_usize(args_obj, "trace_limit")?.unwrap_or(50);
    let limit_cards = optional_usize(args_obj, "limit_cards")?.unwrap_or(30);
    let decisions_limit = optional_usize(args_obj, "decisions_limit")?.unwrap_or(5);
    let evidence_limit = optional_usize(args_obj, "evidence_limit")?.unwrap_or(5);
    let blockers_limit = optional_usize(args_obj, "blockers_limit")?.unwrap_or(5);
    let max_chars = optional_usize(args_obj, "max_chars")?;
    let max_chars = match (context_budget, max_chars) {
        (None, v) => v,
        (Some(budget), None) => Some(budget),
        (Some(budget), Some(explicit)) => Some(explicit.min(budget)),
    };
    let read_only = args_obj
        .get("read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ContextPackArgs {
        workspace,
        warm_archive,
        requested_target,
        requested_ref,
        notes_doc,
        trace_doc,
        graph_doc,
        step,
        agent_id,
        all_lanes,
        notes_limit,
        trace_limit,
        limit_cards,
        decisions_limit,
        evidence_limit,
        blockers_limit,
        max_chars,
        read_only,
    })
}
