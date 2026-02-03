#![forbid(unsafe_code)]

use super::enforce::ResumeSuperBudgetState;
use crate::*;
use serde_json::Value;
use serde_json::json;

fn minimal_capsule_for_budget(capsule: Value) -> Value {
    let Some(obj) = capsule.as_object() else {
        return capsule;
    };
    let mut out = json!({
        "type": obj.get("type").cloned().unwrap_or(Value::Null),
        "version": obj.get("version").cloned().unwrap_or(Value::Null),
        "toolset": obj.get("toolset").cloned().unwrap_or(Value::Null),
        "workspace": obj.get("workspace").cloned().unwrap_or(Value::Null),
        "focus": obj.get("focus").cloned().unwrap_or(Value::Null),
        "where": obj.get("where").cloned().unwrap_or(Value::Null),
        "target": obj.get("target").cloned().unwrap_or(Value::Null),
        "map_action": obj.get("map_action").cloned().unwrap_or(Value::Null),
        "prep_action": obj.get("prep_action").cloned().unwrap_or(Value::Null),
        "action": obj.get("action").cloned().unwrap_or(Value::Null),
        "escalation": obj.get("escalation").cloned().unwrap_or(Value::Null)
    });
    // Preserve openable refs (if present) so portals can still offer navigation anchors even when
    // the envelope is reduced to capsule-only under tight budgets.
    if let Some(refs) = obj.get("refs")
        && let Some(out_obj) = out.as_object_mut()
    {
        out_obj.insert("refs".to_string(), refs.clone());
    }
    out
}

fn minimal_degradation_for_budget(degradation: Value) -> Value {
    let Some(obj) = degradation.as_object() else {
        return degradation;
    };
    json!({
        "signals": obj.get("signals").cloned().unwrap_or(Value::Null),
        "minimal": obj.get("minimal").cloned().unwrap_or(Value::Null)
    })
}

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
            // Ultra-tight budgets: if we've already degraded to capsule-only but the payload
            // still doesn't fit, keep shrinking the capsule instead of falling all the way down
            // to `{}` (minimal signal). This preserves a stable navigation handle (target id)
            // for portals and agents.
            if json_len_chars(value) > limit
                && let Some(obj) = value.as_object()
            {
                let capsule_only = obj.contains_key("capsule")
                    && !obj.contains_key("target")
                    && !obj.contains_key("memory")
                    && !obj.contains_key("signals")
                    && !obj.contains_key("timeline")
                    && !obj.contains_key("steps")
                    && !obj.contains_key("radar");

                if capsule_only {
                    if drop_fields_at(value, &[], &["degradation"]) {
                        return true;
                    }
                    // Ultra-tight budgets: keep the navigation handle (capsule.target.id) by
                    // dropping the redundant top-level truncated flag (budget/warnings already
                    // convey truncation). This avoids falling back to `{}` minimal signal.
                    if json_len_chars(value) > limit && drop_fields_at(value, &[], &["truncated"]) {
                        return true;
                    }
                    if json_len_chars(value) > limit
                        && drop_fields_at(
                            value,
                            &["capsule"],
                            &[
                                "map_action",
                                "map_escalation",
                                "prep_action",
                                "prep_escalation",
                                "action",
                                "escalation",
                            ],
                        )
                    {
                        return true;
                    }
                    if json_len_chars(value) > limit
                        && drop_fields_at(value, &["capsule"], &["where", "workspace", "toolset"])
                    {
                        return true;
                    }
                    if json_len_chars(value) > limit
                        && drop_fields_at(
                            value,
                            &["capsule", "target"],
                            &["kind", "qualified_id", "title", "revision", "parent"],
                        )
                    {
                        return true;
                    }
                    if json_len_chars(value) > limit
                        && drop_fields_at(value, &["capsule"], &["focus", "type", "version"])
                    {
                        return true;
                    }
                    // Navigation invariant: drop `capsule.refs` last. Even under extreme budgets,
                    // keeping a single CARD/JOB/doc@seq ref is often the difference between
                    // "continue instantly" and "hunt through history". If we *still* don't fit,
                    // then refs become expendable.
                    if json_len_chars(value) > limit
                        && drop_fields_at(value, &["capsule"], &["refs"])
                    {
                        return true;
                    }
                }
            }
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
                // Portal lines can still render a meaningful state line because capsule includes a
                // minimal target and focus reference.
                let capsule = obj.remove("capsule");
                let degradation = obj.remove("degradation");
                obj.clear();
                if let Some(capsule) = capsule {
                    obj.insert("capsule".to_string(), minimal_capsule_for_budget(capsule));
                }
                if let Some(degradation) = degradation {
                    obj.insert(
                        "degradation".to_string(),
                        minimal_degradation_for_budget(degradation),
                    );
                }
                obj.insert("truncated".to_string(), Value::Bool(true));
                changed = true;
            }
            changed
        },
    );
}
