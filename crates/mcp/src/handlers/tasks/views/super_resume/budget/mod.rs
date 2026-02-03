#![forbid(unsafe_code)]

mod enforce;
mod fallback;

use crate::*;
use serde_json::{Value, json};

pub(super) struct ResumeSuperBudgetContext<'a> {
    pub events_total: usize,
    pub notes_count: usize,
    pub trace_count: usize,
    pub cards_total: usize,
    pub stats_by_type: &'a std::collections::BTreeMap<String, u64>,
}

impl McpServer {
    pub(super) fn apply_resume_super_budget(
        &mut self,
        result: &mut Value,
        max_chars: Option<usize>,
        ctx: ResumeSuperBudgetContext<'_>,
        degradation_signals: &mut Vec<String>,
        warnings: &mut Vec<Value>,
    ) {
        let ResumeSuperBudgetContext {
            events_total,
            notes_count,
            trace_count,
            cards_total,
            stats_by_type,
        } = ctx;

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            // Navigation safety net: when we know the envelope will be truncated, capture a tiny
            // set of stable openable refs *before* trimming drops the underlying slices.
            if json_len_chars(result) > limit {
                attach_budget_refs_to_capsule(result);
            }
            let mut state = enforce::ResumeSuperBudgetState::new(
                limit,
                events_total,
                notes_count,
                trace_count,
                cards_total,
                stats_by_type,
            );

            enforce::apply(result, &mut state);
            fallback::ensure_limit(result, limit, &mut state);

            if state.truncated {
                degradation_signals.push("budget_truncated".to_string());
            }
            if state.minimal {
                degradation_signals.push("minimal_signal".to_string());
            }

            if let Some(obj) = result
                .get_mut("degradation")
                .and_then(|v| v.as_object_mut())
            {
                obj.insert("signals".to_string(), json!(degradation_signals));
                obj.insert("truncated_fields".to_string(), json!(state.trimmed_fields));
                obj.insert("minimal".to_string(), Value::Bool(state.minimal));
            }

            set_truncated_flag(result, state.truncated);
            warnings.extend(budget_warnings(state.truncated, state.minimal, clamped));
        } else if let Some(obj) = result
            .get_mut("degradation")
            .and_then(|v| v.as_object_mut())
        {
            obj.insert("signals".to_string(), json!(degradation_signals));
        }

        // Keep derived sequential-trace graph consistent with the (possibly trimmed) entries slice.
        // This runs after all budget passes so it reflects the final output shape.
        let entries_snapshot = result
            .get("memory")
            .and_then(|v| v.get("trace"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(sequential) = result
            .get_mut("memory")
            .and_then(|v| v.get_mut("trace"))
            .and_then(|v| v.get_mut("sequential"))
        {
            filter_trace_sequential_graph_to_entries(sequential, &entries_snapshot);
        }

        let cards_snapshot = result
            .get("memory")
            .and_then(|v| v.get("cards"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(engine) = result.get_mut("engine") {
            filter_engine_to_cards(engine, &cards_snapshot);
        }
    }
}

fn attach_budget_refs_to_capsule(result: &mut Value) {
    let refs = collect_resume_open_refs(result);
    if refs.is_empty() {
        return;
    }
    let Some(obj) = result.get_mut("capsule").and_then(|v| v.as_object_mut()) else {
        return;
    };
    obj.insert("refs".to_string(), Value::Array(refs));
}

fn collect_resume_open_refs(result: &Value) -> Vec<Value> {
    // Deterministic, low-noise bounds. This is a navigation safety net for tight budgets.
    const MAX_CARD_REFS: usize = 2;

    let mut out = Vec::<Value>::new();

    let job_id = result
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| s.starts_with("JOB-") && !s.is_empty())
        .map(|s| s.to_string());
    let max_card_refs = if job_id.is_some() { 1 } else { MAX_CARD_REFS };

    if let Some(cards) = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        for card in cards.iter().take(max_card_refs) {
            let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            out.push(json!({ "label": "CARD", "id": id }));
        }
    }

    if let Some(job_id) = job_id.as_deref() {
        out.push(json!({ "label": "JOB", "id": job_id }));
    }

    let notes_doc = result
        .get("reasoning_ref")
        .and_then(|v| v.get("notes_doc"))
        .and_then(|v| v.as_str());
    if let (Some(notes_doc), Some(entries)) = (
        notes_doc,
        result
            .get("memory")
            .and_then(|v| v.get("notes"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array()),
    ) {
        let max_seq = entries
            .iter()
            .filter_map(|e| e.get("seq").and_then(|v| v.as_i64()))
            .max();
        if let Some(seq) = max_seq {
            out.push(json!({ "label": "NOTE", "id": format!("{notes_doc}@{seq}") }));
        }
    }

    let trace_doc = result
        .get("reasoning_ref")
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str());
    if let (Some(trace_doc), Some(entries)) = (
        trace_doc,
        result
            .get("memory")
            .and_then(|v| v.get("trace"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array()),
    ) {
        let max_seq = entries
            .iter()
            .filter_map(|e| e.get("seq").and_then(|v| v.as_i64()))
            .max();
        if let Some(seq) = max_seq {
            out.push(json!({ "label": "TRACE", "id": format!("{trace_doc}@{seq}") }));
        }
    }

    out
}
