#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn optional_string_array(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(arr) = value.as_array() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an array of strings"),
        ));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{key} must be an array of strings"),
            ));
        };
        out.push(s.to_string());
    }
    Ok(Some(out))
}

fn normalize_string_list(raw: Vec<String>) -> Vec<String> {
    use std::collections::HashSet;

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_ascii_lowercase();
        if matches!(lowered.as_str(), "none" | "null" | "n/a" | "na") {
            continue;
        }
        let normalized = trimmed.to_string();
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

pub(crate) fn normalize_optional_string_list(raw: Option<Vec<String>>) -> Option<Vec<String>> {
    raw.map(normalize_string_list)
}

pub(crate) fn normalize_required_string_list(
    raw: Vec<String>,
    field: &str,
) -> Result<Vec<String>, Value> {
    let normalized = normalize_string_list(raw);
    if normalized.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} must not be empty"),
        ));
    }
    Ok(normalized)
}

pub(crate) fn optional_string_values(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(None),
        Some(value) => parse_string_values(Some(value), key).map(Some),
        None => Ok(None),
    }
}

pub(crate) fn optional_string_or_string_array(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => {
            let v = v.trim();
            if v.is_empty() {
                Err(ai_error(
                    "INVALID_INPUT",
                    &format!("{key} must not be empty"),
                ))
            } else {
                Ok(Some(vec![v.to_string()]))
            }
        }
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(s) = item.as_str() else {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        &format!("{key} must be a string or array of strings"),
                    ));
                };
                let s = s.trim();
                if s.is_empty() {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        &format!("{key} must not be empty"),
                    ));
                }
                out.push(s.to_string());
            }
            Ok(Some(out))
        }
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string or array of strings"),
        )),
        None => Ok(None),
    }
}

pub(crate) fn parse_string_values(
    value: Option<&Value>,
    field: &str,
) -> Result<Vec<String>, Value> {
    let Some(value) = value else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} requires a value"),
        ));
    };
    match value {
        Value::String(v) => Ok(vec![v.clone()]),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(v) = item.as_str() else {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        &format!("{field} must be a string array"),
                    ));
                };
                out.push(v.to_string());
            }
            Ok(out)
        }
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} must be a string or array"),
        )),
    }
}

pub(crate) fn apply_list_op(
    target: &mut Vec<String>,
    op_name: &str,
    value: Option<&Value>,
    field: &str,
) -> Result<(), Value> {
    match op_name {
        "set" => {
            *target = parse_string_values(value, field)?;
            Ok(())
        }
        "unset" => {
            target.clear();
            Ok(())
        }
        "append" => {
            let values = parse_string_values(value, field)?;
            for value in values {
                if !target.contains(&value) {
                    target.push(value);
                }
            }
            Ok(())
        }
        "remove" => {
            let values = parse_string_values(value, field)?;
            target.retain(|value| !values.contains(value));
            Ok(())
        }
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} supports set/unset/append/remove"),
        )),
    }
}
