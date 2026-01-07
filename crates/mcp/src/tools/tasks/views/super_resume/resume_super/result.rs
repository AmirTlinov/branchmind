#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::memory::ResumeSuperMemory;
use super::signals::ResumeSuperSignals;
use super::timeline::TimelineEvents;

pub(super) struct ResumeSuperResultArgs<'a> {
    pub(super) workspace: &'a WorkspaceId,
    pub(super) args_obj: &'a serde_json::Map<String, Value>,
    pub(super) notes_cursor: Option<i64>,
    pub(super) notes_limit: usize,
    pub(super) trace_cursor: Option<i64>,
    pub(super) trace_limit: usize,
    pub(super) focus: Option<String>,
    pub(super) focus_previous: Option<String>,
    pub(super) focus_restored: bool,
    pub(super) context: RadarContext,
    pub(super) timeline: TimelineEvents,
    pub(super) signals: ResumeSuperSignals,
    pub(super) memory: ResumeSuperMemory,
    pub(super) include_graph_diff: bool,
    pub(super) graph_diff_payload: Option<Value>,
    pub(super) degradation_signals: &'a [String],
}

fn opt_i64_to_json(value: Option<i64>) -> Value {
    value
        .map(|v| Value::Number(serde_json::Number::from(v)))
        .unwrap_or(Value::Null)
}

pub(super) fn build_resume_super_result(args: ResumeSuperResultArgs<'_>) -> Value {
    let ResumeSuperResultArgs {
        workspace,
        args_obj,
        notes_cursor,
        notes_limit,
        trace_cursor,
        trace_limit,
        focus,
        focus_previous,
        focus_restored,
        context,
        timeline,
        signals,
        memory,
        include_graph_diff,
        graph_diff_payload,
        degradation_signals,
    } = args;

    let notes_count = memory.notes.entries.len();
    let trace_count = memory.trace.entries.len();
    let cards_total = memory.cards.len();
    let decisions_total = signals.decisions.len();
    let evidence_total = signals.evidence.len();
    let blockers_total = signals.blockers.len();

    let trace_sequential = derive_trace_sequential_graph(&memory.trace.entries);

    let mut result = json!( {
        "workspace": workspace.as_str(),
        "requested": {
            "task": args_obj.get("task").cloned().unwrap_or(Value::Null),
            "plan": args_obj.get("plan").cloned().unwrap_or(Value::Null)
        },
        "focus": focus,
        "target": context.target,
        "reasoning_ref": context.reasoning_ref,
        "radar": context.radar,
        "timeline": {
            "limit": timeline.limit,
            "events": events_to_json(timeline.events)
        },
        "signals": {
            "blockers": signals.blockers,
            "decisions": signals.decisions,
            "evidence": signals.evidence,
            "stats": {
                "blockers": blockers_total,
                "decisions": decisions_total,
                "evidence": evidence_total
            }
        },
        "memory": {
            "notes": {
                "entries": memory.notes.entries,
                "pagination": {
                    "cursor": opt_i64_to_json(notes_cursor),
                    "next_cursor": memory.notes.next_cursor,
                    "has_more": memory.notes.has_more,
                    "limit": notes_limit,
                    "count": notes_count
                }
            },
            "trace": {
                "entries": memory.trace.entries,
                "sequential": trace_sequential.unwrap_or(Value::Null),
                "pagination": {
                    "cursor": opt_i64_to_json(trace_cursor),
                    "next_cursor": memory.trace.next_cursor,
                    "has_more": memory.trace.has_more,
                    "limit": trace_limit,
                    "count": trace_count
                }
            },
            "cards": memory.cards,
            "cards_pagination": {
                "cursor": opt_i64_to_json(memory.cards_cursor),
                "next_cursor": memory.cards_next_cursor,
                "has_more": memory.cards_has_more,
                "limit": memory.cards_limit,
                "count": cards_total
            },
            "stats": {
                "cards": cards_total,
                "by_type": memory.stats_by_type
            }
        },
        "degradation": {
            "signals": degradation_signals,
            "truncated_fields": [],
            "minimal": false
        },
        "truncated": false
    });

    if focus_restored && let Some(obj) = result.as_object_mut() {
        obj.insert("focus_restored".to_string(), Value::Bool(true));
        obj.insert(
            "focus_previous".to_string(),
            focus_previous.map(Value::String).unwrap_or(Value::Null),
        );
    }

    if let Some(steps) = context.steps
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("steps".to_string(), steps);
    }

    if include_graph_diff && let Some(obj) = result.as_object_mut() {
        obj.insert(
            "graph_diff".to_string(),
            graph_diff_payload.unwrap_or(Value::Null),
        );
    }

    result
}
