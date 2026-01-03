#![forbid(unsafe_code)]

use super::super::super::StoreError;
use serde_json::Value as JsonValue;

pub(super) fn snapshot_required_str(
    snapshot: &JsonValue,
    field: &str,
) -> Result<String, StoreError> {
    snapshot
        .get(field)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| StoreError::InvalidInput("snapshot missing string field"))
}

pub(super) fn snapshot_required_bool(
    snapshot: &JsonValue,
    field: &str,
) -> Result<bool, StoreError> {
    snapshot
        .get(field)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| StoreError::InvalidInput("snapshot missing boolean field"))
}

pub(super) fn snapshot_optional_string(
    snapshot: &JsonValue,
    field: &str,
) -> Result<Option<String>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        _ => Err(StoreError::InvalidInput("snapshot invalid string field")),
    }
}

pub(super) fn snapshot_optional_i64(
    snapshot: &JsonValue,
    field: &str,
) -> Result<Option<i64>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .ok_or(StoreError::InvalidInput("snapshot invalid integer field"))
            .map(Some),
    }
}

pub(super) fn snapshot_optional_json_string(
    snapshot: &JsonValue,
    field: &str,
) -> Result<Option<String>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => Ok(Some(value.to_string())),
    }
}

pub(super) fn snapshot_required_vec(
    snapshot: &JsonValue,
    field: &str,
) -> Result<Vec<String>, StoreError> {
    let Some(array) = snapshot.get(field).and_then(|v| v.as_array()) else {
        return Err(StoreError::InvalidInput("snapshot missing array field"));
    };
    let mut out = Vec::with_capacity(array.len());
    for value in array {
        let Some(text) = value.as_str() else {
            return Err(StoreError::InvalidInput("snapshot array must be strings"));
        };
        out.push(text.to_string());
    }
    Ok(out)
}
