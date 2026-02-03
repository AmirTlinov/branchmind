#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct ResumeSuperBudgetState<'a> {
    pub(super) limit: usize,
    pub(super) truncated: bool,
    pub(super) minimal: bool,
    pub(super) trimmed_fields: Vec<String>,

    pub(super) events_total: usize,
    pub(super) notes_count: usize,
    pub(super) trace_count: usize,
    pub(super) cards_total: usize,
    pub(super) stats_by_type: &'a std::collections::BTreeMap<String, u64>,
}

impl<'a> ResumeSuperBudgetState<'a> {
    pub(super) fn new(
        limit: usize,
        events_total: usize,
        notes_count: usize,
        trace_count: usize,
        cards_total: usize,
        stats_by_type: &'a std::collections::BTreeMap<String, u64>,
    ) -> Self {
        Self {
            limit,
            truncated: false,
            minimal: false,
            trimmed_fields: Vec::new(),
            events_total,
            notes_count,
            trace_count,
            cards_total,
            stats_by_type,
        }
    }

    pub(super) fn mark_trimmed(&mut self, field: &str) {
        mark_trimmed(&mut self.trimmed_fields, field);
    }
}

pub(super) fn apply(result: &mut Value, state: &mut ResumeSuperBudgetState<'_>) {
    let limit = state.limit;

    if json_len_chars(result) > limit
        && let Some(why) = result
            .get_mut("radar")
            .and_then(|v| v.get_mut("why"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    {
        let mut chars = why.chars();
        let mut shorter = chars.by_ref().take(256).collect::<String>();
        if chars.next().is_some() {
            shorter.push_str("...");
        }
        if let Some(obj) = result.get_mut("radar").and_then(|v| v.as_object_mut()) {
            obj.insert("why".to_string(), Value::String(shorter));
            state.truncated = true;
            state.mark_trimmed("radar.why");
        }
    }

    if json_len_chars(result) > limit {
        let dropped = drop_fields_at(
            result,
            &["target"],
            &["contract_data", "contract", "description"],
        );
        if dropped {
            state.truncated = true;
            state.mark_trimmed("target");
        }
    }

    if json_len_chars(result) > limit {
        let dropped = drop_fields_at(result, &[], &["lane_summary"]);
        if dropped {
            state.truncated = true;
            state.mark_trimmed("lane_summary");
        }
    }

    // Preserve step focus under budget pressure by shrinking the capsule before last-resort
    // capsule-only fallback would drop `step_focus` entirely.
    //
    // Rationale: in smart/focus_only views, `step_focus` is the highest-signal navigation anchor.
    // The capsule is valuable, but its larger optional fields (handoff/radar/counts) should be
    // the first to go.
    if json_len_chars(result) > limit {
        if drop_fields_at(result, &["capsule"], &["map_escalation"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.map_escalation");
        }
        if drop_fields_at(result, &["capsule"], &["prep_escalation"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.prep_escalation");
        }
        if drop_fields_at(result, &["capsule"], &["graph_diff"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.graph_diff");
        }
        if drop_fields_at(result, &["capsule"], &["last_event"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.last_event");
        }
        if drop_fields_at(result, &["capsule"], &["counts"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.counts");
        }
        if drop_fields_at(result, &["capsule"], &["handoff"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.handoff");
        }
        if drop_fields_at(result, &["capsule"], &["radar"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.radar");
        }
        if json_len_chars(result) > limit && drop_fields_at(result, &["capsule"], &["map_action"]) {
            state.truncated = true;
            state.mark_trimmed("capsule.map_action");
        }
    }

    if json_len_chars(result) > limit {
        if compact_event_payloads_at(result, &["timeline", "events"]) {
            state.truncated = true;
            state.mark_trimmed("timeline.events.payload");
        }
        if compact_doc_entries_at(
            result,
            &["memory", "notes", "entries"],
            256,
            true,
            false,
            true,
        ) {
            state.truncated = true;
            state.mark_trimmed("memory.notes.entries");
        }
        if compact_doc_entries_at(
            result,
            &["memory", "trace", "entries"],
            256,
            true,
            false,
            true,
        ) {
            state.truncated = true;
            state.mark_trimmed("memory.trace.entries");
        }
        if compact_card_fields_at(result, &["memory", "cards"], 180, true, true, false) {
            state.truncated = true;
            state.mark_trimmed("memory.cards");
        }
        if compact_card_fields_at(result, &["signals", "decisions"], 160, true, true, false) {
            state.truncated = true;
            state.mark_trimmed("signals.decisions");
        }
        if compact_card_fields_at(result, &["signals", "evidence"], 160, true, true, false) {
            state.truncated = true;
            state.mark_trimmed("signals.evidence");
        }
        if compact_card_fields_at(result, &["signals", "blockers"], 160, true, true, false) {
            state.truncated = true;
            state.mark_trimmed("signals.blockers");
        }
    }

    if trim_array_to_budget(result, &["timeline", "events"], limit, true) {
        state.truncated = true;
        state.mark_trimmed("timeline.events");
    }
    let events_empty = result
        .get("timeline")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if events_empty
        && state.events_total > 0
        && ensure_minimal_list_at(
            result,
            &["timeline", "events"],
            state.events_total,
            "events",
        )
    {
        state.truncated = true;
        state.mark_trimmed("timeline.events");
    }

    if trim_array_to_budget(result, &["memory", "notes", "entries"], limit, true) {
        state.truncated = true;
        state.mark_trimmed("memory.notes.entries");
        refresh_pagination_count(
            result,
            &["memory", "notes", "entries"],
            &["memory", "notes", "pagination"],
        );
    }
    if trim_array_to_budget(result, &["memory", "trace", "entries"], limit, true) {
        state.truncated = true;
        state.mark_trimmed("memory.trace.entries");
        refresh_pagination_count(
            result,
            &["memory", "trace", "entries"],
            &["memory", "trace", "pagination"],
        );
    }
    if trim_array_to_budget(result, &["memory", "cards"], limit, false) {
        state.truncated = true;
        state.mark_trimmed("memory.cards");
        recompute_card_stats_at(result, &["memory", "cards"], &["memory", "stats"]);
        refresh_pagination_count(
            result,
            &["memory", "cards"],
            &["memory", "cards_pagination"],
        );
    }
    if trim_array_to_budget(result, &["signals", "decisions"], limit, false) {
        state.truncated = true;
        state.mark_trimmed("signals.decisions");
    }
    if trim_array_to_budget(result, &["signals", "evidence"], limit, false) {
        state.truncated = true;
        state.mark_trimmed("signals.evidence");
    }
    if trim_array_to_budget(result, &["signals", "blockers"], limit, false) {
        state.truncated = true;
        state.mark_trimmed("signals.blockers");
    }

    if json_len_chars(result) > limit {
        let compacted = compact_stats_by_type_at(result, &["memory", "stats"]);
        if compacted {
            state.truncated = true;
            state.mark_trimmed("memory.stats");
        }
    }
    if json_len_chars(result) > limit {
        let replaced = if let Some(obj) = result.as_object_mut() {
            if obj.contains_key("graph_diff") {
                obj.insert(
                    "graph_diff".to_string(),
                    json!({ "available": false, "reason": "budget" }),
                );
                true
            } else {
                false
            }
        } else {
            false
        };
        if replaced {
            state.truncated = true;
            state.mark_trimmed("graph_diff");
        }
    }

    if json_len_chars(result) > limit {
        let dropped = drop_fields_at(result, &["engine"], &["actions"]);
        if dropped {
            state.truncated = true;
            state.mark_trimmed("engine.actions");
        }
    }
    if json_len_chars(result) > limit {
        let dropped = drop_fields_at(result, &["engine"], &["signals"]);
        if dropped {
            state.truncated = true;
            state.mark_trimmed("engine.signals");
        }
    }
    if json_len_chars(result) > limit {
        let dropped = drop_fields_at(result, &[], &["engine"]);
        if dropped {
            state.truncated = true;
            state.mark_trimmed("engine");
        }
    }

    let notes_empty = result
        .get("memory")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if notes_empty
        && state.notes_count > 0
        && ensure_minimal_list_at(
            result,
            &["memory", "notes", "entries"],
            state.notes_count,
            "notes",
        )
    {
        state.truncated = true;
        state.mark_trimmed("memory.notes.entries");
        set_pagination_total_at(
            result,
            &["memory", "notes", "pagination"],
            state.notes_count,
        );
    }

    let trace_empty = result
        .get("memory")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if trace_empty
        && state.trace_count > 0
        && ensure_minimal_list_at(
            result,
            &["memory", "trace", "entries"],
            state.trace_count,
            "trace",
        )
    {
        state.truncated = true;
        state.mark_trimmed("memory.trace.entries");
        set_pagination_total_at(
            result,
            &["memory", "trace", "pagination"],
            state.trace_count,
        );
    }

    let cards_empty = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if cards_empty
        && state.cards_total > 0
        && ensure_minimal_list_at(result, &["memory", "cards"], state.cards_total, "cards")
    {
        state.truncated = true;
        state.mark_trimmed("memory.cards");
        set_card_stats_at(
            result,
            &["memory", "stats"],
            state.cards_total,
            state.stats_by_type,
        );
        set_pagination_total_at(result, &["memory", "cards_pagination"], state.cards_total);
    }
}
