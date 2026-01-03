#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use bm_core::paths::StepPath;
use serde_json::Value;

pub(crate) fn optional_step_path(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<StepPath>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(raw) = value.as_str() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        ));
    };
    StepPath::parse(raw)
        .map(Some)
        .map_err(|_| ai_error("INVALID_INPUT", &format!("{key} is invalid")))
}

pub(crate) fn parse_task_node_path(raw: &str) -> Result<(StepPath, i64), Value> {
    let raw = raw.trim();
    let Some((step_part, ordinal_part)) = raw.rsplit_once(".t:") else {
        return Err(ai_error(
            "INVALID_INPUT",
            "path must include .t:<ordinal> suffix",
        ));
    };
    let parent_path = StepPath::parse(step_part)
        .map_err(|_| ai_error("INVALID_INPUT", "task node path is invalid"))?;
    let ordinal = ordinal_part
        .parse::<i64>()
        .map_err(|_| ai_error("INVALID_INPUT", "task node ordinal is invalid"))?;
    if ordinal < 0 {
        return Err(ai_error(
            "INVALID_INPUT",
            "task node ordinal must be non-negative",
        ));
    }
    Ok((parent_path, ordinal))
}
