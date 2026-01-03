#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn optional_meta_value(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Value>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ai_error(
                    "INVALID_INPUT",
                    &format!("{key} must not be empty"),
                ));
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(value) => Ok(Some(value)),
                Err(_) => Ok(Some(Value::String(trimmed.to_string()))),
            }
        }
        Some(value) => Ok(Some(value.clone())),
        None => Ok(None),
    }
}

pub(crate) fn merge_meta_with_fields(
    base: Option<Value>,
    fields: Vec<(String, Value)>,
) -> Option<String> {
    let mut out = match base {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("meta".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };

    for (key, value) in fields {
        out.insert(key, value);
    }

    if out.is_empty() {
        None
    } else {
        Some(Value::Object(out).to_string())
    }
}

pub(crate) fn optional_nullable_object_as_json_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Object(_)) => Ok(Some(Some(args.get(key).expect("key exists").to_string()))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an object or null"),
        )),
        None => Ok(None),
    }
}

pub(crate) fn optional_object_as_json_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::Object(_)) => Ok(Some(args.get(key).expect("key exists").to_string())),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an object or null"),
        )),
        None => Ok(None),
    }
}
