#![forbid(unsafe_code)]

use super::ai::{ai_error, ai_error_with, suggest_call};
use bm_core::ids::WorkspaceId;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct ParsedThinkCard {
    pub(crate) card_id: Option<String>,
    pub(crate) card_type: String,
    pub(crate) title: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) status: String,
    pub(crate) tags: Vec<String>,
    pub(crate) meta_value: Value,
}

pub(crate) fn parse_think_card(
    workspace: &WorkspaceId,
    value: Value,
) -> Result<ParsedThinkCard, Value> {
    let raw_obj = match value {
        Value::Object(obj) => obj,
        Value::String(raw) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(ai_error("INVALID_INPUT", "card must not be empty"));
            }
            if raw.starts_with('{') {
                if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) {
                    obj
                } else {
                    let mut obj = serde_json::Map::new();
                    obj.insert("text".to_string(), Value::String(raw.to_string()));
                    obj
                }
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert("text".to_string(), Value::String(raw.to_string()));
                obj
            }
        }
        Value::Null => return Err(ai_error("INVALID_INPUT", "card is required")),
        _ => {
            return Err(ai_error(
                "INVALID_INPUT",
                "card must be an object or string",
            ));
        }
    };

    normalize_think_card(workspace, raw_obj)
}

pub(crate) fn normalize_think_card(
    workspace: &WorkspaceId,
    raw: serde_json::Map<String, Value>,
) -> Result<ParsedThinkCard, Value> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut id: Option<String> = None;
    let mut card_type: Option<String> = None;
    let mut title: Option<String> = None;
    let mut text: Option<String> = None;
    let mut status: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut meta: BTreeMap<String, Value> = BTreeMap::new();

    for (key, value) in raw {
        let key = key.trim().to_ascii_lowercase();
        match key.as_str() {
            "id" | "card_id" => {
                let Some(v) = value.as_str() else {
                    return Err(ai_error("INVALID_INPUT", "card.id must be a string"));
                };
                let v = v.trim();
                if !v.is_empty() {
                    id = Some(v.to_string());
                }
            }
            "type" | "card_type" => {
                let Some(v) = value.as_str() else {
                    return Err(ai_error("INVALID_INPUT", "card.type must be a string"));
                };
                let v = v.trim();
                if !v.is_empty() {
                    card_type = Some(v.to_string());
                }
            }
            "title" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        title = Some(v.to_string());
                    }
                }
            }
            "text" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        text = Some(v.to_string());
                    }
                }
            }
            "status" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        status = Some(v.to_string());
                    }
                }
            }
            "tags" => {
                let mut set = BTreeSet::new();
                match value {
                    Value::Array(arr) => {
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "card.tags must be an array of strings",
                                ));
                            };
                            let s = s.trim();
                            if !s.is_empty() {
                                set.insert(s.to_lowercase());
                            }
                        }
                    }
                    Value::String(s) => {
                        for part in s.split([';', ',']) {
                            let part = part.trim();
                            if !part.is_empty() {
                                set.insert(part.to_lowercase());
                            }
                        }
                    }
                    Value::Null => {}
                    _ => {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            "card.tags must be a string or an array of strings",
                        ));
                    }
                }
                tags = set.into_iter().collect();
            }
            "meta" => match value {
                Value::Object(obj) => {
                    for (k, v) in obj {
                        meta.insert(k, v);
                    }
                }
                Value::String(raw) => {
                    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&raw) {
                        for (k, v) in obj {
                            meta.insert(k, v);
                        }
                    } else {
                        return Err(ai_error("INVALID_INPUT", "card.meta must be an object"));
                    }
                }
                Value::Null => {}
                _ => return Err(ai_error("INVALID_INPUT", "card.meta must be an object")),
            },
            _ => {
                meta.insert(key, value);
            }
        }
    }

    let card_id = id;
    let card_type = card_type.unwrap_or_else(|| "note".to_string());
    if !bm_core::think::is_supported_think_card_type(&card_type) {
        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        return Err(ai_error_with(
            "INVALID_INPUT",
            "Unsupported card.type",
            Some(&format!("Supported: {}", supported.join(", "))),
            vec![suggest_call(
                "think_template",
                "Get a valid card skeleton.",
                "high",
                json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
            )],
        ));
    }

    if title.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
        && text.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
    {
        return Err(ai_error(
            "INVALID_INPUT",
            "card must have at least one of title or text",
        ));
    }

    let status = status.unwrap_or_else(|| "open".to_string());
    let meta_value = Value::Object(meta.into_iter().collect());

    Ok(ParsedThinkCard {
        card_id,
        card_type,
        title,
        text,
        status,
        tags,
        meta_value,
    })
}

pub(crate) fn build_think_card_payload(
    card_id: &str,
    card_type: &str,
    title: Option<&str>,
    text: Option<&str>,
    status: &str,
    tags: &[String],
    meta_value: &Value,
) -> (String, String, String) {
    let normalized = json!({
        "id": card_id,
        "type": card_type,
        "title": title,
        "text": text,
        "status": status,
        "tags": tags,
        "meta": meta_value.clone()
    });
    let payload_json = normalized.to_string();
    let meta_json = json!({
        "source": "think_card",
        "card_id": card_id,
        "type": card_type,
        "status": status,
        "tags": tags,
        "meta": meta_value.clone()
    })
    .to_string();
    let content = text
        .map(|s| s.to_string())
        .or_else(|| title.map(|s| s.to_string()))
        .unwrap_or_default();
    (payload_json, meta_json, content)
}
