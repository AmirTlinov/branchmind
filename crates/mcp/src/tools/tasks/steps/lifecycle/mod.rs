#![forbid(unsafe_code)]
//! Step lifecycle operations (decompose/define/note/verify/done/close_step).

mod close_step;
mod decompose;
mod define;
mod done;
mod note;
mod verify;

use crate::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default)]
struct CheckpointsInput {
    criteria: Option<bool>,
    tests: Option<bool>,
    security: Option<bool>,
    perf: Option<bool>,
    docs: Option<bool>,
}

fn require_step_selector(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<(Option<String>, Option<StepPath>), Value> {
    let step_id = optional_string(args_obj, "step_id")?;
    let path = optional_step_path(args_obj, "path")?;
    if step_id.is_none() && path.is_none() {
        return Err(ai_error("INVALID_INPUT", "step_id or path is required"));
    }
    Ok((step_id, path))
}

fn require_checkpoints(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<CheckpointsInput, Value> {
    let Some(value) = args_obj.get("checkpoints") else {
        return Err(ai_error("INVALID_INPUT", "checkpoints is required"));
    };
    if let Some(mode) = value.as_str() {
        return match mode {
            "all" => Ok(CheckpointsInput {
                criteria: Some(true),
                tests: Some(true),
                security: Some(true),
                perf: Some(true),
                docs: Some(true),
            }),
            "gate" => Ok(CheckpointsInput {
                criteria: Some(true),
                tests: Some(true),
                security: None,
                perf: None,
                docs: None,
            }),
            _ => Err(ai_error(
                "INVALID_INPUT",
                "checkpoints string must be one of: all, gate",
            )),
        };
    }

    let Some(obj) = value.as_object() else {
        return Err(ai_error(
            "INVALID_INPUT",
            "checkpoints must be an object or a string",
        ));
    };

    Ok(CheckpointsInput {
        criteria: checkpoint_confirmed(obj, "criteria")?,
        tests: checkpoint_confirmed(obj, "tests")?,
        security: checkpoint_confirmed(obj, "security")?,
        perf: checkpoint_confirmed(obj, "perf")?,
        docs: checkpoint_confirmed(obj, "docs")?,
    })
}

fn checkpoint_confirmed(
    checkpoints_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Value> {
    let Some(value) = checkpoints_obj.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Bool(v) => Ok(Some(*v)),
        Value::Object(inner) => match inner.get("confirmed") {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(v)) => Ok(Some(*v)),
            Some(_) => Err(ai_error(
                "INVALID_INPUT",
                &format!("checkpoints.{key}.confirmed must be a boolean"),
            )),
        },
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("checkpoints.{key} must be a boolean or an object"),
        )),
    }
}

pub(super) fn checkpoints_suggestion_value(
    criteria: bool,
    tests: bool,
    security: bool,
    perf: bool,
    docs: bool,
) -> Value {
    if criteria && tests && !security && !perf && !docs {
        return Value::String("gate".to_string());
    }

    let mut obj = serde_json::Map::<String, Value>::new();
    if criteria {
        obj.insert("criteria".to_string(), Value::Bool(true));
    }
    if tests {
        obj.insert("tests".to_string(), Value::Bool(true));
    }
    if security {
        obj.insert("security".to_string(), Value::Bool(true));
    }
    if perf {
        obj.insert("perf".to_string(), Value::Bool(true));
    }
    if docs {
        obj.insert("docs".to_string(), Value::Bool(true));
    }

    Value::Object(obj)
}

pub(super) fn checkpoints_suggestion_array_value(
    tests: bool,
    security: bool,
    perf: bool,
    docs: bool,
) -> Value {
    let mut out = Vec::new();
    if tests {
        out.push(Value::String("tests".to_string()));
    }
    if security {
        out.push(Value::String("security".to_string()));
    }
    if perf {
        out.push(Value::String("perf".to_string()));
    }
    if docs {
        out.push(Value::String("docs".to_string()));
    }
    match out.len() {
        0 => Value::Null,
        1 => out.remove(0),
        _ => Value::Array(out),
    }
}
