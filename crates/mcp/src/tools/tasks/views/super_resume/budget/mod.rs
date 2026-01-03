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
    }
}
