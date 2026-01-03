#![forbid(unsafe_code)]

use super::super::super::StoreError;
use bm_core::model::TaskKind;
use serde_json::Value as JsonValue;

pub(in crate::store) fn parse_event_id(event_id: &str) -> Option<i64> {
    let digits = event_id.strip_prefix("evt_")?;
    digits.parse::<i64>().ok()
}

pub(in crate::store) fn parse_plan_or_task_kind(id: &str) -> Result<TaskKind, StoreError> {
    if id.starts_with("PLAN-") {
        Ok(TaskKind::Plan)
    } else if id.starts_with("TASK-") {
        Ok(TaskKind::Task)
    } else {
        Err(StoreError::InvalidInput(
            "task must start with PLAN- or TASK-",
        ))
    }
}

pub(in crate::store) fn parse_json_or_null(value: Option<String>) -> JsonValue {
    match value {
        None => JsonValue::Null,
        Some(raw) => serde_json::from_str(&raw).unwrap_or(JsonValue::Null),
    }
}
