#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(super) struct FrontierTotals {
    pub(super) hypotheses_total: usize,
    pub(super) questions_total: usize,
    pub(super) subgoals_total: usize,
    pub(super) tests_total: usize,
}

pub(super) fn enforce(result: &mut Value, max_chars: usize, totals: FrontierTotals) -> Vec<Value> {
    let (limit, clamped) = clamp_budget_max(max_chars);
    let mut truncated = false;
    let mut minimal = false;
    let mut forced_minimal = false;

    if json_len_chars(result) > limit {
        truncated |=
            compact_card_fields_at(result, &["frontier", "hypotheses"], 160, true, true, false);
        truncated |=
            compact_card_fields_at(result, &["frontier", "questions"], 160, true, true, false);
        truncated |=
            compact_card_fields_at(result, &["frontier", "subgoals"], 160, true, true, false);
        truncated |= compact_card_fields_at(result, &["frontier", "tests"], 160, true, true, false);
    }
    if json_len_chars(result) > limit {
        truncated |= trim_array_to_budget(result, &["frontier", "hypotheses"], limit, false);
        truncated |= trim_array_to_budget(result, &["frontier", "questions"], limit, false);
        truncated |= trim_array_to_budget(result, &["frontier", "subgoals"], limit, false);
        truncated |= trim_array_to_budget(result, &["frontier", "tests"], limit, false);
    }
    if json_len_chars(result) > limit {
        if ensure_minimal_list_at(
            result,
            &["frontier", "hypotheses"],
            totals.hypotheses_total,
            "hypotheses",
        ) {
            truncated = true;
            minimal = true;
        }
        if ensure_minimal_list_at(
            result,
            &["frontier", "questions"],
            totals.questions_total,
            "questions",
        ) {
            truncated = true;
            minimal = true;
        }
        if ensure_minimal_list_at(
            result,
            &["frontier", "subgoals"],
            totals.subgoals_total,
            "subgoals",
        ) {
            truncated = true;
            minimal = true;
        }
        if ensure_minimal_list_at(result, &["frontier", "tests"], totals.tests_total, "tests") {
            truncated = true;
            minimal = true;
        }
    }

    let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |value| {
        let mut changed = false;
        if json_len_chars(value) > limit {
            changed |= minimalize_cards_at(value, &["frontier", "hypotheses"]);
            changed |= minimalize_cards_at(value, &["frontier", "questions"]);
            changed |= minimalize_cards_at(value, &["frontier", "subgoals"]);
            changed |= minimalize_cards_at(value, &["frontier", "tests"]);
        }
        if json_len_chars(value) > limit {
            changed |= retain_one_at(value, &["frontier", "hypotheses"], true);
            changed |= retain_one_at(value, &["frontier", "questions"], true);
            changed |= retain_one_at(value, &["frontier", "subgoals"], true);
            changed |= retain_one_at(value, &["frontier", "tests"], true);
        }
        if json_len_chars(value) > limit {
            if ensure_minimal_list_at(
                value,
                &["frontier", "hypotheses"],
                totals.hypotheses_total,
                "hypotheses",
            ) {
                changed = true;
            }
            if ensure_minimal_list_at(
                value,
                &["frontier", "questions"],
                totals.questions_total,
                "questions",
            ) {
                changed = true;
            }
            if ensure_minimal_list_at(
                value,
                &["frontier", "subgoals"],
                totals.subgoals_total,
                "subgoals",
            ) {
                changed = true;
            }
            if ensure_minimal_list_at(value, &["frontier", "tests"], totals.tests_total, "tests") {
                changed = true;
            }
        }
        if json_len_chars(value) > limit {
            *value = minimal_frontier_value(
                limit,
                totals.hypotheses_total,
                totals.questions_total,
                totals.subgoals_total,
                totals.tests_total,
            );
            forced_minimal = true;
            changed = true;
        }
        changed
    });

    if forced_minimal {
        truncated = true;
        minimal = true;
    }
    set_truncated_flag(result, truncated);
    budget_warnings(truncated, minimal, clamped)
}
