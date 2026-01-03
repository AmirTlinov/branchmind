#![forbid(unsafe_code)]

use crate::*;
use bm_core::ids::WorkspaceId;
use serde_json::Value;

pub(super) struct ContextPackArgs {
    pub workspace: WorkspaceId,
    pub requested_target: Option<String>,
    pub requested_ref: Option<String>,
    pub notes_doc: Option<String>,
    pub trace_doc: Option<String>,
    pub graph_doc: Option<String>,
    pub notes_limit: usize,
    pub trace_limit: usize,
    pub limit_cards: usize,
    pub decisions_limit: usize,
    pub evidence_limit: usize,
    pub blockers_limit: usize,
    pub max_chars: Option<usize>,
    pub read_only: bool,
}

pub(super) fn parse_context_pack_args(args: Value) -> Result<ContextPackArgs, Value> {
    let Some(args_obj) = args.as_object() else {
        return Err(ai_error("INVALID_INPUT", "arguments must be an object"));
    };
    let workspace = require_workspace(args_obj)?;

    let requested_target = args_obj
        .get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let requested_ref = optional_string(args_obj, "ref")?;
    let notes_doc = optional_string(args_obj, "notes_doc")?;
    let trace_doc = optional_string(args_obj, "trace_doc")?;
    let graph_doc = optional_string(args_obj, "graph_doc")?;

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
    let read_only = args_obj
        .get("read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ContextPackArgs {
        workspace,
        requested_target,
        requested_ref,
        notes_doc,
        trace_doc,
        graph_doc,
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
