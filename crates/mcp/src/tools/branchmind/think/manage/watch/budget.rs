#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(super) struct WatchTotals {
    pub(super) candidates_total: usize,
    pub(super) trace_total: usize,
    pub(super) frontier_hypotheses_total: usize,
    pub(super) frontier_questions_total: usize,
    pub(super) frontier_subgoals_total: usize,
    pub(super) frontier_tests_total: usize,
}

pub(super) fn enforce(result: &mut Value, max_chars: usize, totals: WatchTotals) -> Vec<Value> {
    let (limit, clamped) = clamp_budget_max(max_chars);
    let mut truncated = false;
    let mut minimal = false;

    let candidates_total = totals.candidates_total;
    let trace_count = totals.trace_total;
    let hypotheses_total = totals.frontier_hypotheses_total;
    let questions_total = totals.frontier_questions_total;
    let subgoals_total = totals.frontier_subgoals_total;
    let tests_total = totals.frontier_tests_total;

    if json_len_chars(result) > limit {
        truncated |= compact_card_fields_at(result, &["candidates"], 180, true, true, false);
        for path in [
            &["frontier", "tests"][..],
            &["frontier", "subgoals"][..],
            &["frontier", "questions"][..],
            &["frontier", "hypotheses"][..],
        ] {
            truncated |= compact_card_fields_at(result, path, 180, true, true, false);
        }
        truncated |= compact_doc_entries_at(result, &["trace", "entries"], 256, true, false, true);
    }
    truncated |= trim_array_to_budget(result, &["candidates"], limit, false);

    let candidates_empty = result
        .get("candidates")
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if candidates_empty
        && candidates_total > 0
        && ensure_minimal_list_at(result, &["candidates"], candidates_total, "candidates")
    {
        truncated = true;
        minimal = true;
    }

    if json_len_chars(result) > limit {
        let trimmed_trace = trim_array_to_budget(result, &["trace", "entries"], limit, true);
        if trimmed_trace {
            refresh_trace_pagination_count(result);
            truncated = true;
        }
    }

    let trace_empty = result
        .get("trace")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if trace_empty
        && trace_count > 0
        && ensure_minimal_list_at(result, &["trace", "entries"], trace_count, "trace")
    {
        truncated = true;
        minimal = true;
        set_pagination_total_at(result, &["trace", "pagination"], trace_count);
    }

    if json_len_chars(result) > limit {
        for path in [
            &["frontier", "tests"][..],
            &["frontier", "subgoals"][..],
            &["frontier", "questions"][..],
            &["frontier", "hypotheses"][..],
        ] {
            if json_len_chars(result) <= limit {
                break;
            }
            truncated |= trim_array_to_budget(result, path, limit, false);
        }
    }
    for (path, total, label) in [
        (
            &["frontier", "hypotheses"][..],
            hypotheses_total,
            "hypotheses",
        ),
        (&["frontier", "questions"][..], questions_total, "questions"),
        (&["frontier", "subgoals"][..], subgoals_total, "subgoals"),
        (&["frontier", "tests"][..], tests_total, "tests"),
    ] {
        let empty = result
            .get(path[0])
            .and_then(|v| v.get(path[1]))
            .and_then(|v| v.as_array())
            .map(|arr| arr.is_empty())
            .unwrap_or(true);
        if empty && total > 0 && ensure_minimal_list_at(result, path, total, label) {
            truncated = true;
            minimal = true;
        }
    }

    if json_len_chars(result) > limit {
        let compacted = compact_trace_pagination(result);
        if compacted {
            refresh_trace_pagination_count(result);
            truncated = true;
        }
    }

    let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |value| {
        let mut changed = false;
        if json_len_chars(value) > limit {
            changed |= compact_card_fields_at(value, &["candidates"], 120, true, true, true);
            for path in [
                &["frontier", "tests"][..],
                &["frontier", "subgoals"][..],
                &["frontier", "questions"][..],
                &["frontier", "hypotheses"][..],
            ] {
                changed |= compact_card_fields_at(value, path, 120, true, true, true);
            }
            changed |= compact_doc_entries_at(value, &["trace", "entries"], 128, true, true, true);
        }
        if json_len_chars(value) > limit {
            if minimalize_cards_at(value, &["candidates"]) {
                changed = true;
            }
            for path in [
                &["frontier", "tests"][..],
                &["frontier", "subgoals"][..],
                &["frontier", "questions"][..],
                &["frontier", "hypotheses"][..],
            ] {
                if minimalize_cards_at(value, path) {
                    changed = true;
                }
            }
            if minimalize_doc_entries_at(value, &["trace", "entries"]) {
                changed = true;
            }
        }
        if json_len_chars(value) > limit {
            if retain_one_at(value, &["candidates"], true) {
                changed = true;
            }
            if retain_one_at(value, &["trace", "entries"], true) {
                changed = true;
                refresh_trace_pagination_count(value);
            }
        }
        if json_len_chars(value) > limit {
            if ensure_minimal_list_at(value, &["candidates"], candidates_total, "candidates") {
                changed = true;
            }
            if ensure_minimal_list_at(value, &["trace", "entries"], trace_count, "trace") {
                changed = true;
                set_pagination_total_at(value, &["trace", "pagination"], trace_count);
            }
            for (path, total, label) in [
                (
                    &["frontier", "hypotheses"][..],
                    hypotheses_total,
                    "hypotheses",
                ),
                (&["frontier", "questions"][..], questions_total, "questions"),
                (&["frontier", "subgoals"][..], subgoals_total, "subgoals"),
                (&["frontier", "tests"][..], tests_total, "tests"),
            ] {
                if ensure_minimal_list_at(value, path, total, label) {
                    changed = true;
                }
            }
        }
        if json_len_chars(value) > limit {
            changed |= drop_fields_at(
                value,
                &["frontier"],
                &["tests", "subgoals", "questions", "hypotheses"],
            );
        }
        if json_len_chars(value) > limit {
            changed |= drop_fields_at(value, &["trace"], &["pagination"]);
        }
        if json_len_chars(value) > limit {
            changed |= drop_fields_at(value, &[], &["trace"]);
        }
        changed
    });

    set_truncated_flag(result, truncated);
    budget_warnings(truncated, minimal, clamped)
}
