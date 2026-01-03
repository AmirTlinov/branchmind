#![forbid(unsafe_code)]

use serde_json::Value;

use super::super::shared::{drop_fields_at, get_array_mut_at, get_object_mut_at, truncate_string};

pub(crate) fn compact_card_fields_at(
    value: &mut Value,
    path: &[&str],
    max_text: usize,
    drop_meta: bool,
    drop_tags: bool,
    drop_status: bool,
) -> bool {
    let Some(cards) = get_array_mut_at(value, path) else {
        return false;
    };
    let mut changed = false;
    for card in cards.iter_mut() {
        if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
            let shorter = truncate_string(title, max_text);
            if shorter != title {
                if let Some(obj) = card.as_object_mut() {
                    obj.insert("title".to_string(), Value::String(shorter));
                }
                changed = true;
            }
        }
        if let Some(text) = card.get("text").and_then(|v| v.as_str()) {
            let shorter = truncate_string(text, max_text);
            if shorter != text {
                if let Some(obj) = card.as_object_mut() {
                    obj.insert("text".to_string(), Value::String(shorter));
                }
                changed = true;
            }
        }
        if drop_meta
            && let Some(obj) = card.as_object_mut()
            && obj.contains_key("meta")
        {
            obj.insert("meta".to_string(), Value::Null);
            changed = true;
        }
        if drop_tags
            && let Some(obj) = card.as_object_mut()
            && obj.contains_key("tags")
        {
            obj.insert("tags".to_string(), Value::Array(Vec::new()));
            changed = true;
        }
        if drop_status {
            changed |= drop_fields_at(card, &[], &["status"]);
        }
    }
    changed
}

pub(crate) fn compact_card_value(
    card: &mut Value,
    max_text: usize,
    drop_meta: bool,
    drop_tags: bool,
    drop_status: bool,
) -> bool {
    let Some(obj) = card.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(title) = obj.get("title").and_then(|v| v.as_str()) {
        let shorter = truncate_string(title, max_text);
        if shorter != title {
            obj.insert("title".to_string(), Value::String(shorter));
            changed = true;
        }
    }
    if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        let shorter = truncate_string(text, max_text);
        if shorter != text {
            obj.insert("text".to_string(), Value::String(shorter));
            changed = true;
        }
    }
    if drop_meta && obj.contains_key("meta") {
        obj.insert("meta".to_string(), Value::Null);
        changed = true;
    }
    if drop_tags && obj.contains_key("tags") {
        obj.insert("tags".to_string(), Value::Array(Vec::new()));
        changed = true;
    }
    if drop_status && obj.remove("status").is_some() {
        changed = true;
    }
    changed
}

pub(crate) fn compact_stats_by_type(value: &mut Value) -> bool {
    let Some(stats) = value.get_mut("stats").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    if stats.contains_key("by_type") {
        stats.insert("by_type".to_string(), Value::Object(serde_json::Map::new()));
        return true;
    }
    false
}

pub(crate) fn compact_stats_by_type_at(value: &mut Value, stats_path: &[&str]) -> bool {
    let Some(stats) = get_object_mut_at(value, stats_path) else {
        return false;
    };
    if stats.contains_key("by_type") {
        stats.insert("by_type".to_string(), Value::Object(serde_json::Map::new()));
        return true;
    }
    false
}
