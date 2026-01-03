#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn optional_i64(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<i64>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{key} must be an integer"))),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an integer"),
        )),
    }
}

pub(crate) fn optional_usize(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<usize>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n.as_u64().map(|v| v as usize).map(Some).ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{key} must be a positive integer"),
            )
        }),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a positive integer"),
        )),
    }
}
