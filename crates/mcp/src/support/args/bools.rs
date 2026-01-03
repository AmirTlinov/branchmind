#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use serde_json::Value;

pub(crate) fn optional_bool(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Bool(v) => Ok(Some(*v)),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a boolean"),
        )),
    }
}
