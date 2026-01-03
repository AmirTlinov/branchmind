#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::shared::{get_array_at, get_object_mut_at};

pub(crate) fn recompute_card_stats(value: &mut Value, cards_key: &str) -> usize {
    let (count, by_type) = {
        let Some(cards) = value.get(cards_key).and_then(|v| v.as_array()) else {
            return 0;
        };
        let mut by_type = BTreeMap::<String, u64>::new();
        for card in cards {
            if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                *by_type.entry(ty.to_string()).or_insert(0) += 1;
            }
        }
        (cards.len(), by_type)
    };

    if let Some(stats) = value.get_mut("stats").and_then(|v| v.as_object_mut()) {
        stats.insert(
            "cards".to_string(),
            Value::Number(serde_json::Number::from(count as u64)),
        );
        stats.insert("by_type".to_string(), json!(by_type));
    }

    count
}

pub(crate) fn recompute_card_stats_at(
    value: &mut Value,
    cards_path: &[&str],
    stats_path: &[&str],
) -> usize {
    let (count, by_type) = {
        let Some(cards) = get_array_at(value, cards_path) else {
            return 0;
        };
        let mut by_type = BTreeMap::<String, u64>::new();
        for card in cards {
            if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                *by_type.entry(ty.to_string()).or_insert(0) += 1;
            }
        }
        (cards.len(), by_type)
    };

    if let Some(stats) = get_object_mut_at(value, stats_path) {
        stats.insert(
            "cards".to_string(),
            Value::Number(serde_json::Number::from(count as u64)),
        );
        stats.insert("by_type".to_string(), json!(by_type));
    }

    count
}

pub(crate) fn set_card_stats(
    value: &mut Value,
    total: usize,
    by_type: &BTreeMap<String, u64>,
) -> bool {
    let Some(stats) = value.get_mut("stats").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    stats.insert(
        "cards".to_string(),
        Value::Number(serde_json::Number::from(total as u64)),
    );
    stats.insert("by_type".to_string(), json!(by_type));
    true
}

pub(crate) fn set_card_stats_at(
    value: &mut Value,
    stats_path: &[&str],
    total: usize,
    by_type: &BTreeMap<String, u64>,
) -> bool {
    let Some(stats) = get_object_mut_at(value, stats_path) else {
        return false;
    };
    stats.insert(
        "cards".to_string(),
        Value::Number(serde_json::Number::from(total as u64)),
    );
    stats.insert("by_type".to_string(), json!(by_type));
    true
}
