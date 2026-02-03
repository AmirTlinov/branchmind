#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct ResumePackBudgetContext {
    pub events_total: usize,
    pub decisions_total: usize,
    pub evidence_total: usize,
    pub blockers_total: usize,
}

impl McpServer {
    pub(super) fn apply_resume_pack_budget(
        &mut self,
        result: &mut Value,
        max_chars: Option<usize>,
        ctx: ResumePackBudgetContext,
        warnings: &mut Vec<Value>,
    ) {
        let ResumePackBudgetContext {
            events_total,
            decisions_total,
            evidence_total,
            blockers_total,
        } = ctx;

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(result) > limit {
                truncated |= compact_event_payloads_at(result, &["timeline", "events"]);
                truncated |= compact_card_fields_at(
                    result,
                    &["signals", "decisions"],
                    160,
                    true,
                    true,
                    false,
                );
                truncated |= compact_card_fields_at(
                    result,
                    &["signals", "evidence"],
                    160,
                    true,
                    true,
                    false,
                );
            }
            truncated |= trim_array_to_budget(result, &["timeline", "events"], limit, true);
            truncated |= trim_array_to_budget(result, &["signals", "decisions"], limit, false);
            truncated |= trim_array_to_budget(result, &["signals", "evidence"], limit, false);
            let events_empty = result
                .get("timeline")
                .and_then(|v| v.get("events"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if events_empty
                && events_total > 0
                && ensure_minimal_list_at(result, &["timeline", "events"], events_total, "events")
            {
                truncated = true;
                minimal = true;
            }
            let decisions_empty = result
                .get("signals")
                .and_then(|v| v.get("decisions"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if decisions_empty
                && decisions_total > 0
                && ensure_minimal_list_at(
                    result,
                    &["signals", "decisions"],
                    decisions_total,
                    "decisions",
                )
            {
                truncated = true;
                minimal = true;
                set_signal_stats(result, blockers_total, decisions_total, evidence_total);
            }
            let evidence_empty = result
                .get("signals")
                .and_then(|v| v.get("evidence"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if evidence_empty
                && evidence_total > 0
                && ensure_minimal_list_at(
                    result,
                    &["signals", "evidence"],
                    evidence_total,
                    "evidence",
                )
            {
                truncated = true;
                minimal = true;
                set_signal_stats(result, blockers_total, decisions_total, evidence_total);
            }
            let blockers_empty = result
                .get("signals")
                .and_then(|v| v.get("blockers"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if blockers_empty
                && blockers_total > 0
                && ensure_minimal_list_at(
                    result,
                    &["signals", "blockers"],
                    blockers_total,
                    "blockers",
                )
            {
                truncated = true;
                minimal = true;
                set_signal_stats(result, blockers_total, decisions_total, evidence_total);
            }
            if json_len_chars(result) > limit {
                let mut removed_any = false;
                if let Some(first) = result
                    .get_mut("steps")
                    .and_then(|v| v.as_object_mut())
                    .and_then(|steps| steps.get_mut("first_open"))
                    .and_then(|v| v.as_object_mut())
                {
                    for key in [
                        "criteria_confirmed",
                        "tests_confirmed",
                        "security_confirmed",
                        "perf_confirmed",
                        "docs_confirmed",
                    ] {
                        removed_any |= first.remove(key).is_some();
                    }
                }
                truncated |= removed_any;
            }
            if json_len_chars(result) > limit {
                if compact_radar_for_budget(result) {
                    truncated = true;
                }
                if compact_target_for_budget(result) {
                    truncated = true;
                }
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["radar"], &["why"]);
            }

            let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |value| {
                let mut changed = false;
                if json_len_chars(value) > limit {
                    changed |= compact_event_payloads_at(value, &["timeline", "events"]);
                    changed |= compact_card_fields_at(
                        value,
                        &["signals", "decisions"],
                        120,
                        true,
                        true,
                        true,
                    );
                    changed |= compact_card_fields_at(
                        value,
                        &["signals", "evidence"],
                        120,
                        true,
                        true,
                        true,
                    );
                }
                if json_len_chars(value) > limit {
                    if minimalize_cards_at(value, &["signals", "decisions"]) {
                        changed = true;
                    }
                    if minimalize_cards_at(value, &["signals", "evidence"]) {
                        changed = true;
                    }
                }
                if json_len_chars(value) > limit {
                    changed |= retain_one_at(value, &["timeline", "events"], true);
                    changed |= retain_one_at(value, &["signals", "decisions"], true);
                    changed |= retain_one_at(value, &["signals", "evidence"], true);
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &[], &["steps"]);
                }
                if json_len_chars(value) > limit {
                    if ensure_minimal_list_at(
                        value,
                        &["timeline", "events"],
                        events_total,
                        "events",
                    ) {
                        changed = true;
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "decisions"],
                        decisions_total,
                        "decisions",
                    ) {
                        changed = true;
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "evidence"],
                        evidence_total,
                        "evidence",
                    ) {
                        changed = true;
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "blockers"],
                        blockers_total,
                        "blockers",
                    ) {
                        changed = true;
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                    }
                }
                if json_len_chars(value) > limit {
                    changed |=
                        drop_fields_at(value, &["signals"], &["blockers", "decisions", "evidence"]);
                }
                if json_len_chars(value) > limit {
                    changed |=
                        drop_fields_at(value, &["radar"], &["verify", "next", "blockers", "why"]);
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &["timeline"], &["events"]);
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &[], &["timeline"]);
                }
                changed
            });

            set_truncated_flag(result, truncated);
            warnings.extend(budget_warnings(truncated, minimal, clamped));
        }
    }
}
