#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::read::{ContextPackDocs, ContextPackTotals};

pub(super) struct ContextPackRenderGraph {
    pub cards: Vec<Value>,
    pub decisions: Vec<Value>,
    pub evidence: Vec<Value>,
    pub blockers: Vec<Value>,
    pub by_type: std::collections::BTreeMap<String, u64>,
}

pub(super) struct ContextPackRenderArgs<'a> {
    pub workspace: &'a WorkspaceId,
    pub requested_target: Option<String>,
    pub requested_ref: Option<String>,
    pub scope: ReasoningScope,
    pub docs: ContextPackDocs,
    pub graph: ContextPackRenderGraph,
    pub notes_limit: usize,
    pub trace_limit: usize,
    pub bridge: Option<Value>,
}

pub(super) fn render_context_pack(args: ContextPackRenderArgs<'_>) -> Value {
    let ContextPackRenderArgs {
        workspace,
        requested_target,
        requested_ref,
        scope,
        docs,
        graph,
        notes_limit,
        trace_limit,
        bridge,
    } = args;

    let ContextPackDocs { notes, trace } = docs;
    let sequential = derive_trace_sequential_graph(&trace.entries).unwrap_or(Value::Null);
    let ContextPackTotals {
        notes_count,
        trace_count,
        cards_total: card_count,
        decisions_total,
        evidence_total,
        blockers_total,
    } = ContextPackTotals {
        notes_count: notes.entries.len(),
        trace_count: trace.entries.len(),
        cards_total: graph.cards.len(),
        decisions_total: graph.decisions.len(),
        evidence_total: graph.evidence.len(),
        blockers_total: graph.blockers.len(),
    };

    let mut result = json!({
        "workspace": workspace.as_str(),
        "requested": {
            "target": requested_target.map(Value::String).unwrap_or(Value::Null),
            "ref": requested_ref.map(Value::String).unwrap_or(Value::Null)
        },
        "branch": scope.branch,
        "docs": {
            "notes": scope.notes_doc,
            "trace": scope.trace_doc,
            "graph": scope.graph_doc
        },
        "notes": {
            "entries": notes.entries,
            "pagination": {
                "cursor": Value::Null,
                "next_cursor": notes.next_cursor,
                "has_more": notes.has_more,
                "limit": notes_limit,
                "count": notes_count
            }
        },
        "trace": {
            "entries": trace.entries,
            "sequential": sequential,
            "pagination": {
                "cursor": Value::Null,
                "next_cursor": trace.next_cursor,
                "has_more": trace.has_more,
                "limit": trace_limit,
                "count": trace_count
            }
        },
        "signals": {
            "blockers": graph.blockers,
            "decisions": graph.decisions,
            "evidence": graph.evidence,
            "stats": {
                "blockers": blockers_total,
                "decisions": decisions_total,
                "evidence": evidence_total
            }
        },
        "stats": {
            "cards": card_count,
            "by_type": graph.by_type
        },
        "cards": graph.cards,
        "truncated": false
    });

    if let Some(bridge) = bridge
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("bridge".to_string(), bridge);
    }

    result
}
