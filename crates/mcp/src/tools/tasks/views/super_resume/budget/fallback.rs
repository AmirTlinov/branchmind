#![forbid(unsafe_code)]

use super::enforce::ResumeSuperBudgetState;
use crate::*;
use serde_json::Value;
use serde_json::json;

pub(super) fn ensure_limit(
    result: &mut serde_json::Value,
    limit: usize,
    state: &mut ResumeSuperBudgetState<'_>,
) {
    let _used = ensure_budget_limit(
        result,
        limit,
        &mut state.truncated,
        &mut state.minimal,
        |value| {
            let mut changed = false;
            if json_len_chars(value) > limit {
                changed |= compact_event_payloads_at(value, &["timeline", "events"]);
                changed |= compact_doc_entries_at(
                    value,
                    &["memory", "notes", "entries"],
                    120,
                    true,
                    true,
                    true,
                );
                changed |= compact_doc_entries_at(
                    value,
                    &["memory", "trace", "entries"],
                    120,
                    true,
                    true,
                    true,
                );
                changed |=
                    compact_card_fields_at(value, &["memory", "cards"], 120, true, true, true);
                changed |=
                    compact_card_fields_at(value, &["signals", "decisions"], 120, true, true, true);
                changed |=
                    compact_card_fields_at(value, &["signals", "evidence"], 120, true, true, true);
                changed |=
                    compact_card_fields_at(value, &["signals", "blockers"], 120, true, true, true);
            }
            if json_len_chars(value) > limit {
                changed |= minimalize_doc_entries_at(value, &["memory", "notes", "entries"]);
                changed |= minimalize_doc_entries_at(value, &["memory", "trace", "entries"]);
                changed |= minimalize_cards_at(value, &["memory", "cards"]);
                if minimalize_cards_at(value, &["signals", "decisions"]) {
                    changed = true;
                }
                if minimalize_cards_at(value, &["signals", "evidence"]) {
                    changed = true;
                }
                if minimalize_cards_at(value, &["signals", "blockers"]) {
                    changed = true;
                }
            }
            if json_len_chars(value) > limit {
                changed |= retain_one_at(value, &["timeline", "events"], true);
                changed |= retain_one_at(value, &["memory", "notes", "entries"], true);
                changed |= retain_one_at(value, &["memory", "trace", "entries"], true);
                changed |= retain_one_at(value, &["memory", "cards"], true);
                changed |= retain_one_at(value, &["signals", "decisions"], true);
                changed |= retain_one_at(value, &["signals", "evidence"], true);
                changed |= retain_one_at(value, &["signals", "blockers"], true);
                refresh_pagination_count(
                    value,
                    &["memory", "notes", "entries"],
                    &["memory", "notes", "pagination"],
                );
                refresh_pagination_count(
                    value,
                    &["memory", "trace", "entries"],
                    &["memory", "trace", "pagination"],
                );
                recompute_card_stats_at(value, &["memory", "cards"], &["memory", "stats"]);
                refresh_signal_stats(value);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(
                    value,
                    &["target"],
                    &["contract_data", "contract", "description"],
                );
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &[], &["lane_summary"]);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &[], &["steps"]);
            }
            if json_len_chars(value) > limit {
                changed |=
                    drop_fields_at(value, &["radar"], &["verify", "next", "blockers", "why"]);
            }
            if json_len_chars(value) > limit
                && let Some(obj) = value.as_object_mut()
                && obj.contains_key("graph_diff")
            {
                obj.insert(
                    "graph_diff".to_string(),
                    json!({ "available": false, "reason": "budget" }),
                );
                changed = true;
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &["timeline"], &["events"]);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &["engine"], &["actions", "signals"]);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &[], &["engine"]);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(
                    value,
                    &["memory"],
                    &["notes", "trace", "cards", "cards_pagination"],
                );
            }
            if json_len_chars(value) > limit {
                changed |=
                    drop_fields_at(value, &["signals"], &["blockers", "decisions", "evidence"]);
            }
            if json_len_chars(value) > limit {
                changed |= drop_fields_at(value, &[], &["memory", "signals", "timeline"]);
            }
            if json_len_chars(value) > limit {
                let Some(obj) = value.as_object_mut() else {
                    return changed;
                };

                // Last-resort fallback: keep the HUD capsule so the agent still has coordinates
                // and a next action even under aggressive budgets.
                let capsule = obj.remove("capsule");
                let degradation = obj.remove("degradation");
                obj.clear();
                if let Some(capsule) = capsule {
                    obj.insert("capsule".to_string(), capsule);
                }
                if let Some(degradation) = degradation {
                    obj.insert("degradation".to_string(), degradation);
                }
                obj.insert("truncated".to_string(), Value::Bool(true));
                changed = true;
            }
            changed
        },
    );
}
