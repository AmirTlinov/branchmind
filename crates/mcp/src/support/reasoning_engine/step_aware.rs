#![forbid(unsafe_code)]

use serde_json::Value;

use super::derive::derive_reasoning_engine;
use super::types::{EngineLimits, EngineScope};
use super::util::card_has_tag;

fn merge_engine_arrays(
    primary: Vec<Value>,
    secondary: Vec<Value>,
    limit: usize,
    key_fn: fn(&Value) -> Option<String>,
) -> (Vec<Value>, bool) {
    if limit == 0 {
        return (Vec::new(), false);
    }

    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut truncated = false;

    for item in primary.into_iter().chain(secondary.into_iter()) {
        if out.len() >= limit {
            truncated = true;
            break;
        }
        let Some(key) = key_fn(&item) else {
            // Non-conforming items are kept (deterministically) and don't participate in dedupe.
            out.push(item);
            continue;
        };
        if seen.insert(key) {
            out.push(item);
        }
    }

    (out, truncated)
}

fn signal_key(item: &Value) -> Option<String> {
    let code = item.get("code").and_then(|v| v.as_str())?;
    let message = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
    Some(format!("{code}\n{message}"))
}

fn action_key(item: &Value) -> Option<String> {
    let kind = item.get("kind").and_then(|v| v.as_str())?;
    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    Some(format!("{kind}\n{title}"))
}

fn step_selector_from_step_tag(step_tag: &str) -> Option<String> {
    let step = step_tag
        .strip_prefix("step:")
        .unwrap_or(step_tag)
        .trim()
        .to_string();
    if step.is_empty() {
        return None;
    }
    Some(step.to_ascii_uppercase())
}

fn apply_step_scope_to_engine_calls(
    engine_obj: &mut serde_json::Map<String, Value>,
    step_tag: &str,
) {
    let Some(step_selector) = step_selector_from_step_tag(step_tag) else {
        return;
    };
    let Some(actions) = engine_obj.get_mut("actions").and_then(|v| v.as_array_mut()) else {
        return;
    };

    for action in actions {
        let Some(action_obj) = action.as_object_mut() else {
            continue;
        };
        let Some(calls) = action_obj.get_mut("calls").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for call in calls {
            let Some(call_obj) = call.as_object_mut() else {
                continue;
            };
            let target = call_obj.get("target").and_then(|v| v.as_str());
            if !matches!(target, Some("think_card" | "think_pipeline")) {
                continue;
            }
            let Some(params) = call_obj.get_mut("params").and_then(|v| v.as_object_mut()) else {
                continue;
            };
            params
                .entry("step".to_string())
                .or_insert_with(|| Value::String(step_selector.clone()));
        }
    }
}

pub(crate) fn derive_reasoning_engine_step_aware(
    scope: EngineScope<'_>,
    cards: &[Value],
    edges: &[Value],
    trace_entries: &[Value],
    focus_step_tag: Option<&str>,
    limits: EngineLimits,
) -> Option<Value> {
    let step_tag = focus_step_tag.map(str::trim).filter(|t| !t.is_empty());
    if step_tag.is_none() {
        return derive_reasoning_engine(scope, cards, edges, trace_entries, limits);
    }
    let step_tag = step_tag.unwrap();

    let step_cards = cards
        .iter()
        .filter(|card| card_has_tag(card, step_tag))
        .cloned()
        .collect::<Vec<_>>();
    if step_cards.is_empty() {
        return derive_reasoning_engine(scope, cards, edges, trace_entries, limits);
    }

    let mut step_ids = std::collections::BTreeSet::<String>::new();
    for card in &step_cards {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            step_ids.insert(id.to_string());
        }
    }

    let step_edges = edges
        .iter()
        .filter(|edge| {
            let from = edge.get("from").and_then(|v| v.as_str());
            let to = edge.get("to").and_then(|v| v.as_str());
            from.is_some_and(|id| step_ids.contains(id))
                && to.is_some_and(|id| step_ids.contains(id))
        })
        .cloned()
        .collect::<Vec<_>>();

    let step_engine =
        derive_reasoning_engine(scope, &step_cards, &step_edges, trace_entries, limits);
    let global_engine = derive_reasoning_engine(scope, cards, edges, trace_entries, limits);

    let (mut global, step) = match (global_engine, step_engine) {
        (None, None) => return None,
        (Some(global), None) => return Some(global),
        (None, Some(step)) => return Some(step),
        (Some(global), Some(step)) => (global, step),
    };

    let Some(global_obj) = global.as_object_mut() else {
        return Some(global);
    };
    let step_obj = step.as_object().cloned().unwrap_or_default();

    let limit_signals = limits.signals_limit;
    let limit_actions = limits.actions_limit;

    let step_signals = step_obj
        .get("signals")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let global_signals = global_obj
        .get("signals")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let step_actions = step_obj
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let global_actions = global_obj
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let (merged_signals, truncated_signals) =
        merge_engine_arrays(step_signals, global_signals, limit_signals, signal_key);
    let (merged_actions, truncated_actions) =
        merge_engine_arrays(step_actions, global_actions, limit_actions, action_key);

    global_obj.insert("signals".to_string(), Value::Array(merged_signals));
    global_obj.insert("actions".to_string(), Value::Array(merged_actions));
    global_obj.insert(
        "truncated".to_string(),
        Value::Bool(
            global_obj
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || truncated_signals
                || truncated_actions,
        ),
    );
    global_obj.insert("mode".to_string(), Value::String("step_aware".to_string()));
    global_obj.insert("step_tag".to_string(), Value::String(step_tag.to_string()));
    apply_step_scope_to_engine_calls(global_obj, step_tag);

    Some(Value::Object(global_obj.clone()))
}
