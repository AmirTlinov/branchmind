#![forbid(unsafe_code)]

use serde_json::Value;

pub(crate) fn set_signal_stats(
    value: &mut Value,
    blockers: usize,
    decisions: usize,
    evidence: usize,
) -> bool {
    let Some(signals) = value.get_mut("signals").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let Some(stats) = signals.get_mut("stats").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    stats.insert(
        "blockers".to_string(),
        Value::Number(serde_json::Number::from(blockers as u64)),
    );
    stats.insert(
        "decisions".to_string(),
        Value::Number(serde_json::Number::from(decisions as u64)),
    );
    stats.insert(
        "evidence".to_string(),
        Value::Number(serde_json::Number::from(evidence as u64)),
    );
    true
}

pub(crate) fn refresh_signal_stats(value: &mut Value) -> bool {
    let Some(signals) = value.get_mut("signals").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let blockers = signals
        .get("blockers")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let decisions = signals
        .get("decisions")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let evidence = signals
        .get("evidence")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let Some(stats) = signals.get_mut("stats").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    stats.insert(
        "blockers".to_string(),
        Value::Number(serde_json::Number::from(blockers as u64)),
    );
    stats.insert(
        "decisions".to_string(),
        Value::Number(serde_json::Number::from(decisions as u64)),
    );
    stats.insert(
        "evidence".to_string(),
        Value::Number(serde_json::Number::from(evidence as u64)),
    );
    true
}
