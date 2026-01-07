#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn optional_agent_id(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let raw = match value {
        Value::Null => return Ok(None),
        Value::String(v) => v.to_string(),
        _ => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{key} must be a string"),
            ));
        }
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    if trimmed.len() > 64 {
        return Err(ai_error("INVALID_INPUT", &format!("{key} is too long")));
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must start with an ASCII letter or digit"),
        ));
    }
    for ch in chars {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            continue;
        }
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} contains invalid characters"),
        ));
    }
    Ok(Some(trimmed.to_ascii_lowercase()))
}
