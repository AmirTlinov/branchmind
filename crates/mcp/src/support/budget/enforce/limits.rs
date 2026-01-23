use serde_json::{Value, json};

use super::super::shared::{attach_budget, json_len_chars, payload_len_chars};

pub(crate) fn ensure_budget_limit<F>(
    value: &mut Value,
    max_chars: usize,
    truncated: &mut bool,
    minimal: &mut bool,
    mut fallback: F,
) -> usize
where
    F: FnMut(&mut Value) -> bool,
{
    let mut used = payload_len_chars(value);
    if used > max_chars {
        // Ultra-tight budgets may require multiple "drop one more field" passes to preserve a
        // stable capsule/navigation handle (instead of falling all the way down to `{}`).
        //
        // Keep the loop deterministic and bounded, but allow more passes when max_chars is tiny.
        let max_passes = if max_chars <= 200 { 24 } else { 6 };
        for _ in 0..max_passes {
            if !fallback(value) {
                break;
            }
            *truncated = true;
            used = payload_len_chars(value);
            if used <= max_chars {
                break;
            }
        }
    }
    if used > max_chars && max_chars > 0 {
        *truncated = true;
        *minimal = true;
        *value = minimal_signal_value(max_chars);
    }
    attach_budget(value, max_chars, *truncated)
}

pub(crate) fn minimal_signal_value(max_chars: usize) -> Value {
    let candidates = [
        json!({"signal": "minimal"}),
        json!({"signal": "min"}),
        json!({"min": true}),
        json!({}),
    ];
    for candidate in candidates {
        if json_len_chars(&candidate) <= max_chars {
            return candidate;
        }
    }
    json!({})
}

pub(crate) fn minimal_frontier_value(
    max_chars: usize,
    hypotheses_total: usize,
    questions_total: usize,
    subgoals_total: usize,
    tests_total: usize,
) -> Value {
    let total_all = hypotheses_total + questions_total + subgoals_total + tests_total;
    let candidates = [
        json!({
            "frontier": {
                "hypotheses": [{ "type": "summary", "label": "hypotheses", "count": hypotheses_total }],
                "questions": [{ "type": "summary", "label": "questions", "count": questions_total }],
                "subgoals": [{ "type": "summary", "label": "subgoals", "count": subgoals_total }],
                "tests": [{ "type": "summary", "label": "tests", "count": tests_total }]
            },
            "truncated": true
        }),
        json!({
            "frontier": {
                "hypotheses": [{ "count": hypotheses_total }],
                "questions": [{ "count": questions_total }],
                "subgoals": [{ "count": subgoals_total }],
                "tests": [{ "count": tests_total }]
            },
            "truncated": true
        }),
        json!({ "frontier": { "count": total_all }, "truncated": true }),
    ];
    for candidate in candidates {
        if json_len_chars(&candidate) <= max_chars {
            return candidate;
        }
    }
    minimal_signal_value(max_chars)
}

pub(crate) fn minimal_next_value(max_chars: usize, candidate: Option<Value>) -> Value {
    if let Some(candidate) = candidate {
        let candidate_obj = json!({ "candidate": candidate, "truncated": true });
        if json_len_chars(&candidate_obj) <= max_chars {
            return candidate_obj;
        }
        let candidate_only = json!({ "candidate": candidate });
        if json_len_chars(&candidate_only) <= max_chars {
            return candidate_only;
        }
    }
    minimal_signal_value(max_chars)
}
