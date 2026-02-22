#![forbid(unsafe_code)]

use bm_storage::StoreError;
use serde_json::{Value, json};

pub(crate) fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::UnknownBranch => "Unknown branch".to_string(),
        StoreError::BranchAlreadyExists => "Branch already exists".to_string(),
        StoreError::BranchCycle => "Branch base cycle".to_string(),
        StoreError::BranchDepthExceeded => "Branch base depth exceeded".to_string(),
    }
}

pub(crate) fn warning(code: &str, message: &str, recovery: &str) -> Value {
    json!({
        "code": code,
        "message": message,
        "recovery": recovery
    })
}

pub(crate) fn ai_ok_with_warnings(
    intent: &str,
    result: Value,
    warnings: Vec<Value>,
    refs: Vec<Value>,
) -> Value {
    json!({
        "success": true,
        "intent": intent,
        "result": result,
        "warnings": warnings,
        "refs": refs,
        "error": null
    })
}

pub(crate) fn ai_ok(intent: &str, result: Value) -> Value {
    ai_ok_with_warnings(intent, result, Vec::new(), Vec::new())
}

pub(crate) fn ai_error_with(
    code: &str,
    message: &str,
    recovery: Option<&str>,
    refs: Vec<Value>,
) -> Value {
    let mut error_obj = serde_json::Map::new();
    error_obj.insert("code".to_string(), Value::String(code.to_string()));
    error_obj.insert(
        "message".to_string(),
        Value::String(message.trim().to_string()),
    );
    if let Some(recovery) = recovery {
        error_obj.insert(
            "recovery".to_string(),
            Value::String(recovery.trim().to_string()),
        );
    }

    json!({
        "success": false,
        "intent": "error",
        "result": {},
        "warnings": [],
        "refs": refs,
        "error": Value::Object(error_obj)
    })
}
