#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn ensure_nonempty_doc(doc: &Option<String>, key: &str) -> Result<(), Value> {
    if let Some(doc) = doc.as_ref()
        && doc.trim().is_empty()
    {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(())
}

pub(crate) fn require_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Value> {
    let Some(v) = args.get(key).and_then(|v| v.as_str()) else {
        return Err(ai_error("INVALID_INPUT", &format!("{key} is required")));
    };
    Ok(v.to_string())
}

pub(crate) fn optional_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(v) => Ok(Some(v.to_string())),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        )),
    }
}

pub(crate) fn optional_non_null_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::String(v)) => Ok(Some(v.to_string())),
        Some(Value::Null) => Err(ai_error("INVALID_INPUT", &format!("{key} cannot be null"))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        )),
        None => Ok(None),
    }
}

pub(crate) fn optional_nullable_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(v)) => Ok(Some(Some(v.to_string()))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string or null"),
        )),
        None => Ok(None),
    }
}
