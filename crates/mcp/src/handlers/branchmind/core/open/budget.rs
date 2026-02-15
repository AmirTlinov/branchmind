#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::compact::compact_open_result;
use super::compact_budget::trim_compact_open_result_for_budget;

#[derive(Clone, Debug)]
struct OpenStableBinding {
    id: String,
    kind: Option<String>,
    workspace: Option<String>,
    reasoning_ref: Option<Value>,
}

impl OpenStableBinding {
    fn capture(open_id: &str, v: &Value) -> Self {
        let kind = v
            .get("kind")
            .and_then(|vv| vv.as_str())
            .map(|s| s.to_string());
        let workspace = v
            .get("workspace")
            .and_then(|vv| vv.as_str())
            .map(|s| s.to_string());
        let reasoning_ref = v.get("reasoning_ref").cloned().filter(|vv| !vv.is_null());
        Self {
            id: open_id.to_string(),
            kind,
            workspace,
            reasoning_ref,
        }
    }
}

pub(super) fn apply_open_budget_and_verbosity(
    result: Value,
    open_id: &str,
    verbosity: ResponseVerbosity,
    max_chars: Option<usize>,
    warnings: &mut Vec<Value>,
) -> Value {
    match verbosity {
        ResponseVerbosity::Compact => {
            let mut out = compact_open_result(open_id, &result);
            apply_open_budget_compact(&mut out, open_id, max_chars, warnings);
            out
        }
        ResponseVerbosity::Full => {
            let mut out = result;
            apply_open_budget_full(&mut out, open_id, max_chars, warnings);
            out
        }
    }
}

pub(super) fn dedupe_warnings_by_code(warnings: &mut Vec<Value>) {
    let mut seen = std::collections::HashSet::<String>::new();
    warnings.retain(|w| {
        let Some(code) = w.get("code").and_then(|v| v.as_str()) else {
            return true;
        };
        if code.trim().is_empty() {
            return true;
        }
        seen.insert(code.to_string())
    });
}

fn apply_open_budget_compact(
    result: &mut Value,
    open_id: &str,
    max_chars: Option<usize>,
    warnings: &mut Vec<Value>,
) {
    if let Some(limit) = max_chars {
        let (limit, clamped) = clamp_budget_max(limit);
        let binding = OpenStableBinding::capture(open_id, result);
        let pre_truncated = result
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut truncated = pre_truncated;
        let mut minimal = false;

        let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |v| {
            trim_compact_open_result_for_budget(v, limit)
        });

        if minimal {
            *result = open_minimal_value(limit, &binding);
            let _used = attach_budget(result, limit, true);
            truncated = true;
        }

        set_truncated_flag(result, truncated);
        warnings.extend(budget_warnings(truncated, minimal, clamped));
        return;
    }

    if result.get("budget").is_none() {
        // Some open kinds don't go through the budget-aware super-resume machinery.
        // Ensure budget is visible even in compact output (UX invariant).
        let used = json_len_chars(result);
        let (limit, _clamped) = clamp_budget_max(used);
        let _used = attach_budget(result, limit, false);
    }
}

fn apply_open_budget_full(
    result: &mut Value,
    open_id: &str,
    max_chars: Option<usize>,
    warnings: &mut Vec<Value>,
) {
    if let Some(limit) = max_chars {
        let (limit, clamped) = clamp_budget_max(limit);
        let binding = OpenStableBinding::capture(open_id, result);
        let pre_truncated = result
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut truncated = pre_truncated;
        let mut minimal = false;

        let _used = ensure_budget_limit(result, limit, &mut truncated, &mut minimal, |v| {
            trim_open_full_result_for_budget(v, limit)
        });

        if minimal {
            *result = open_minimal_value(limit, &binding);
            let _used = attach_budget(result, limit, true);
            truncated = true;
        }

        set_truncated_flag(result, truncated);
        warnings.extend(budget_warnings(truncated, minimal, clamped));
        return;
    }

    if result.get("budget").is_none() {
        // For open calls without explicit budgets, keep the payload stable but still report the
        // effective size to the caller (cheap drift guard + UX).
        let used = json_len_chars(result);
        let (limit, _clamped) = clamp_budget_max(used);
        let _used = attach_budget(result, limit, false);
    }
}

fn open_minimal_value(max_chars: usize, binding: &OpenStableBinding) -> Value {
    let mut candidates = Vec::<Value>::new();
    if let (Some(workspace), Some(kind), Some(reasoning_ref)) = (
        binding.workspace.as_deref(),
        binding.kind.as_deref(),
        binding.reasoning_ref.as_ref(),
    ) {
        candidates.push(json!({
            "signal": "minimal",
            "workspace": workspace,
            "kind": kind,
            "id": binding.id.as_str(),
            "reasoning_ref": reasoning_ref,
            "truncated": true
        }));
    }
    if let (Some(workspace), Some(kind)) = (binding.workspace.as_deref(), binding.kind.as_deref()) {
        candidates.push(json!({
            "signal": "minimal",
            "workspace": workspace,
            "kind": kind,
            "id": binding.id.as_str(),
            "truncated": true
        }));
    }
    if let (Some(kind), Some(workspace)) = (binding.kind.as_deref(), binding.workspace.as_deref()) {
        // Same as above but allow the "binding-only" shape (no signal) for very tight budgets.
        candidates.push(json!({
            "workspace": workspace,
            "kind": kind,
            "id": binding.id.as_str(),
            "truncated": true
        }));
    }
    if let Some(kind) = binding.kind.as_deref() {
        candidates.push(json!({
            "kind": kind,
            "id": binding.id.as_str(),
            "truncated": true
        }));
    }
    candidates.push(json!({
        "id": binding.id.as_str(),
        "truncated": true
    }));
    candidates.push(json!({
        "signal": "minimal",
        "truncated": true
    }));

    for candidate in candidates {
        if json_len_chars(&candidate) <= max_chars {
            return candidate;
        }
    }

    minimal_signal_value(max_chars)
}

fn trim_open_full_result_for_budget(v: &mut Value, limit: usize) -> bool {
    let mut changed = false;

    // Prefer dropping heaviest user-provided strings first (open(target) often carries large `description` / `why`).
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["target"], &["description"]);
        changed |= drop_fields_at(v, &["capsule", "radar"], &["why"]);
        changed |= drop_fields_at(v, &["content", "radar"], &["why"]);
    }

    // Keep `open` navigable under budgets: trim capsule redundancy before dropping whole blocks.
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["capsule"], &["counts"]);
        changed |= drop_fields_at(v, &["capsule"], &["handoff"]);
        changed |= drop_fields_at(v, &["capsule"], &["last_event"]);
        changed |= drop_fields_at(v, &["capsule"], &["radar"]);
        changed |= drop_fields_at(v, &["capsule"], &["where"]);
        changed |= drop_fields_at(v, &["capsule"], &["target"]);
        changed |= drop_fields_at(v, &["capsule"], &["graph_diff"]);
    }

    // Prefer dropping payload-heavy fields first (legacy open budget logic, extended).
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["steps"]);
        changed |= drop_fields_at(v, &[], &["slice_task"]);
        changed |= drop_fields_at(v, &[], &["slice_plan_spec"]);
        changed |= drop_fields_at(v, &["card"], &["text"]);
        changed |= drop_fields_at(v, &["entry"], &["content"]);
        changed |= drop_fields_at(v, &["entry"], &["payload"]);
        changed |= drop_fields_at(v, &[], &["prompt"]);
        changed |= drop_fields_at(v, &["content", "memory"], &["cards"]);
        changed |= drop_fields_at(v, &["content", "memory", "trace"], &["sequential"]);
        changed |= drop_fields_at(v, &["content", "memory", "trace"], &["entries"]);
        changed |= drop_fields_at(v, &["content", "memory", "notes"], &["entries"]);
        changed |= drop_fields_at(v, &["content", "timeline"], &["events"]);
        changed |= compact_card_fields_at(v, &["cards"], 160, true, false, true);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["card"], &["meta"]);
        changed |= drop_fields_at(v, &["entry"], &["meta"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["step_focus"]);
        changed |= drop_fields_at(v, &[], &["degradation"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["edges"], &["supports", "blocks"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["card"], &["tags"]);
    }
    if json_len_chars(v) > limit {
        if let Some(events) = v.get_mut("events").and_then(|vv| vv.as_array_mut()) {
            for ev in events.iter_mut() {
                if let Some(msg) = ev.get("message").and_then(|vv| vv.as_str()) {
                    let msg = truncate_string(&redact_text(msg), 140);
                    if let Some(obj) = ev.as_object_mut() {
                        obj.insert("message".to_string(), Value::String(msg));
                    }
                }
            }
            changed = true;
        }
        if json_len_chars(v) > limit
            && let Some(events) = v.get_mut("events").and_then(|vv| vv.as_array_mut())
        {
            for ev in events.iter_mut() {
                if let Some(obj) = ev.as_object_mut() {
                    obj.remove("refs");
                }
            }
            changed = true;
        }
    }
    if json_len_chars(v) > limit {
        let (_used, truncated_cards) = enforce_graph_list_budget(v, "cards", limit);
        if truncated_cards {
            changed = true;
        }
        let (_used, truncated_events) = enforce_graph_list_budget(v, "events", limit);
        if truncated_events {
            changed = true;
        }
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["cards"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["events"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["content"], &["signals"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["content"], &["steps"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["content"], &["radar"]);
    }

    // Last resort before minimal: drop bulky navigation blocks, but keep binding + reasoning_ref.
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["capsule"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["target"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["anchor"], &["description", "refs"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["stats"]);
    }

    changed
}
