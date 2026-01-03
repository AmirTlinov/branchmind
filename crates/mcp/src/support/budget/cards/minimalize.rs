#![forbid(unsafe_code)]

use serde_json::{Value, json};

use super::super::shared::{get_array_mut_at, truncate_string};

pub(crate) fn minimalize_card_value(card: &mut Value) -> bool {
    let Some(obj) = card.as_object() else {
        return false;
    };
    if obj.is_empty() {
        return false;
    }
    let mut out = serde_json::Map::new();
    if let Some(id) = obj.get("id") {
        out.insert("id".to_string(), id.clone());
    }
    if let Some(ty) = obj.get("type") {
        out.insert("type".to_string(), ty.clone());
    }
    if let Some(title) = obj.get("title").and_then(|v| v.as_str()) {
        out.insert(
            "title".to_string(),
            Value::String(truncate_string(title, 80)),
        );
    } else if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        out.insert("text".to_string(), Value::String(truncate_string(text, 80)));
    }
    if let Some(status) = obj.get("status") {
        out.insert("status".to_string(), status.clone());
    }
    *card = Value::Object(out);
    true
}

pub(crate) fn minimalize_cards_at(value: &mut Value, path: &[&str]) -> bool {
    let Some(cards) = get_array_mut_at(value, path) else {
        return false;
    };
    if cards.is_empty() {
        return false;
    }
    let mut changed = false;
    for card in cards.iter_mut() {
        let mut out = serde_json::Map::new();
        if let Some(id) = card.get("id") {
            out.insert("id".to_string(), id.clone());
        }
        if let Some(ty) = card.get("type") {
            out.insert("type".to_string(), ty.clone());
        }
        if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
            out.insert(
                "title".to_string(),
                Value::String(truncate_string(title, 80)),
            );
        } else if let Some(text) = card.get("text").and_then(|v| v.as_str()) {
            out.insert("text".to_string(), Value::String(truncate_string(text, 80)));
        }
        if let Some(status) = card.get("status") {
            out.insert("status".to_string(), status.clone());
        }
        *card = Value::Object(out);
        changed = true;
    }
    changed
}

pub(crate) fn ensure_minimal_list_at(
    value: &mut Value,
    path: &[&str],
    total: usize,
    label: &str,
) -> bool {
    if total == 0 {
        return false;
    }
    let Some(cards) = get_array_mut_at(value, path) else {
        return false;
    };
    if !cards.is_empty() {
        return false;
    }
    cards.push(json!({
        "type": "summary",
        "label": label,
        "count": total
    }));
    true
}
