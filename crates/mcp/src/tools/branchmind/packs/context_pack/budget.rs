#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct ContextPackBudgetContext<'a> {
    pub notes_count: usize,
    pub trace_count: usize,
    pub cards_total: usize,
    pub decisions_total: usize,
    pub evidence_total: usize,
    pub blockers_total: usize,
    pub stats_by_type: &'a std::collections::BTreeMap<String, u64>,
}

impl McpServer {
    pub(super) fn apply_context_pack_budget(
        &mut self,
        result: &mut Value,
        max_chars: Option<usize>,
        ctx: ContextPackBudgetContext<'_>,
        warnings: &mut Vec<Value>,
    ) {
        let ContextPackBudgetContext {
            notes_count,
            trace_count,
            cards_total,
            decisions_total,
            evidence_total,
            blockers_total,
            stats_by_type,
        } = ctx;

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(result) > limit {
                truncated |=
                    compact_doc_entries_at(result, &["notes", "entries"], 256, true, false, true);
                truncated |=
                    compact_doc_entries_at(result, &["trace", "entries"], 256, true, false, true);
                truncated |= compact_card_fields_at(result, &["cards"], 180, true, true, false);
                truncated |= compact_card_fields_at(
                    result,
                    &["signals", "decisions"],
                    180,
                    true,
                    true,
                    false,
                );
                truncated |= compact_card_fields_at(
                    result,
                    &["signals", "evidence"],
                    180,
                    true,
                    true,
                    false,
                );
                truncated |= compact_card_fields_at(
                    result,
                    &["signals", "blockers"],
                    180,
                    true,
                    true,
                    false,
                );
            }

            // Keep HUD useful under aggressive budgets: drop optional heavy derived fields first.
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &[], &["lane_summary"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["trace"], &["sequential"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["engine"], &["actions"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["engine"], &["signals"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &[], &["engine"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["capsule", "why"], &["signals"]);
            }
            if json_len_chars(result) > limit {
                truncated |= drop_fields_at(result, &["capsule", "next"], &["backup"]);
            }

            let before_cards = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, cards_truncated) = enforce_graph_list_budget(result, "cards", limit);
            let after_cards = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            truncated |= cards_truncated;
            if after_cards < before_cards {
                if after_cards == 0 && cards_total > 0 {
                    set_card_stats(result, cards_total, stats_by_type);
                } else {
                    recompute_card_stats(result, "cards");
                }
            }

            if json_len_chars(result) > limit {
                truncated |= trim_array_to_budget(result, &["notes", "entries"], limit, true);
                refresh_pagination_count(result, &["notes", "entries"], &["notes", "pagination"]);
                let notes_empty = result
                    .get("notes")
                    .and_then(|v| v.get("entries"))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.is_empty())
                    .unwrap_or(true);
                if notes_empty
                    && notes_count > 0
                    && ensure_minimal_list_at(result, &["notes", "entries"], notes_count, "notes")
                {
                    truncated = true;
                    minimal = true;
                    set_pagination_total_at(result, &["notes", "pagination"], notes_count);
                }
            }
            if json_len_chars(result) > limit {
                truncated |= trim_array_to_budget(result, &["trace", "entries"], limit, true);
                refresh_pagination_count(result, &["trace", "entries"], &["trace", "pagination"]);
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
            }
            if json_len_chars(result) > limit {
                let decisions_trimmed =
                    trim_array_to_budget(result, &["signals", "decisions"], limit, false);
                let evidence_trimmed =
                    trim_array_to_budget(result, &["signals", "evidence"], limit, false);
                let blockers_trimmed =
                    trim_array_to_budget(result, &["signals", "blockers"], limit, false);
                if decisions_trimmed || evidence_trimmed || blockers_trimmed {
                    truncated = true;
                    let signals_empty = ["decisions", "evidence", "blockers"].iter().all(|key| {
                        result
                            .get("signals")
                            .and_then(|v| v.get(*key))
                            .and_then(|v| v.as_array())
                            .map(|a| a.is_empty())
                            .unwrap_or(true)
                    });
                    if signals_empty
                        && (decisions_total > 0 || evidence_total > 0 || blockers_total > 0)
                    {
                        set_signal_stats(result, blockers_total, decisions_total, evidence_total);
                    } else {
                        refresh_signal_stats(result);
                    }
                }
            }
            if json_len_chars(result) > limit && compact_stats_by_type(result) {
                truncated = true;
            }

            let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |value| {
                let mut changed = false;
                if json_len_chars(value) > limit {
                    changed |=
                        compact_doc_entries_at(value, &["notes", "entries"], 128, true, true, true);
                    changed |=
                        compact_doc_entries_at(value, &["trace", "entries"], 128, true, true, true);
                    changed |= compact_card_fields_at(value, &["cards"], 120, true, true, true);
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
                    changed |= compact_card_fields_at(
                        value,
                        &["signals", "blockers"],
                        120,
                        true,
                        true,
                        true,
                    );
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &[], &["lane_summary"]);
                    changed |= drop_fields_at(value, &["trace"], &["sequential"]);
                    changed |= drop_fields_at(value, &["engine"], &["actions"]);
                    changed |= drop_fields_at(value, &["engine"], &["signals"]);
                    changed |= drop_fields_at(value, &[], &["engine"]);
                    changed |= drop_fields_at(value, &["capsule", "why"], &["signals"]);
                    changed |= drop_fields_at(value, &["capsule", "next"], &["backup"]);
                }
                if json_len_chars(value) > limit {
                    changed |= minimalize_doc_entries_at(value, &["notes", "entries"]);
                    changed |= minimalize_doc_entries_at(value, &["trace", "entries"]);
                    changed |= minimalize_cards_at(value, &["cards"]);
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
                    changed |= retain_one_at(value, &["notes", "entries"], true);
                    changed |= retain_one_at(value, &["trace", "entries"], true);
                    changed |= retain_one_at(value, &["cards"], true);
                    refresh_pagination_count(
                        value,
                        &["notes", "entries"],
                        &["notes", "pagination"],
                    );
                    refresh_pagination_count(
                        value,
                        &["trace", "entries"],
                        &["trace", "pagination"],
                    );
                    recompute_card_stats(value, "cards");
                    if retain_one_at(value, &["signals", "decisions"], true)
                        || retain_one_at(value, &["signals", "evidence"], true)
                        || retain_one_at(value, &["signals", "blockers"], true)
                    {
                        refresh_signal_stats(value);
                        changed = true;
                    }
                }
                if json_len_chars(value) > limit {
                    if ensure_minimal_list_at(value, &["cards"], cards_total, "cards") {
                        set_card_stats(value, cards_total, stats_by_type);
                        changed = true;
                    }
                    if ensure_minimal_list_at(value, &["notes", "entries"], notes_count, "notes") {
                        set_pagination_total_at(value, &["notes", "pagination"], notes_count);
                        changed = true;
                    }
                    if ensure_minimal_list_at(value, &["trace", "entries"], trace_count, "trace") {
                        set_pagination_total_at(value, &["trace", "pagination"], trace_count);
                        changed = true;
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "decisions"],
                        decisions_total,
                        "decisions",
                    ) {
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                        changed = true;
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "evidence"],
                        evidence_total,
                        "evidence",
                    ) {
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                        changed = true;
                    }
                    if ensure_minimal_list_at(
                        value,
                        &["signals", "blockers"],
                        blockers_total,
                        "blockers",
                    ) {
                        set_signal_stats(value, blockers_total, decisions_total, evidence_total);
                        changed = true;
                    }
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &["notes"], &["pagination", "branch", "doc"]);
                    changed |= drop_fields_at(value, &["trace"], &["pagination", "branch", "doc"]);
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &[], &["docs"]);
                }
                if json_len_chars(value) > limit {
                    changed |=
                        drop_fields_at(value, &["signals"], &["blockers", "decisions", "evidence"]);
                }
                if json_len_chars(value) > limit {
                    changed |= drop_fields_at(value, &[], &["signals"]);
                }
                if json_len_chars(value) > limit {
                    let Some(obj) = value.as_object_mut() else {
                        return changed;
                    };
                    let capsule = obj.remove("capsule");
                    obj.clear();
                    if let Some(capsule) = capsule {
                        obj.insert("capsule".to_string(), capsule);
                    }
                    obj.insert("truncated".to_string(), Value::Bool(true));
                    changed = true;
                }
                changed
            });

            set_truncated_flag(result, truncated);
            warnings.extend(budget_warnings(truncated, minimal, clamped));
        }
    }
}
