#![forbid(unsafe_code)]

use crate::support::time::now_rfc3339;
use bm_storage::StoreError;
use serde_json::{Value, json};

pub(crate) fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::RevisionMismatch { expected, actual } => {
            format!("Revision mismatch: expected={expected} actual={actual}")
        }
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::UnknownBranch => "Unknown branch".to_string(),
        StoreError::UnknownConflict => "Unknown conflict".to_string(),
        StoreError::ConflictAlreadyResolved => "Conflict already resolved".to_string(),
        StoreError::MergeNotSupported => "Merge not supported".to_string(),
        StoreError::BranchAlreadyExists => "Branch already exists".to_string(),
        StoreError::BranchCycle => "Branch base cycle".to_string(),
        StoreError::BranchDepthExceeded => "Branch base depth exceeded".to_string(),
        StoreError::StepNotFound => "Step not found".to_string(),
        StoreError::CheckpointsNotConfirmed {
            criteria,
            tests,
            security,
            perf,
            docs,
        } => format!(
            "Checkpoints not confirmed: criteria={criteria} tests={tests} security={security} perf={perf} docs={docs}"
        ),
        StoreError::ProofMissing {
            tests,
            security,
            perf,
            docs,
        } => format!("Proof missing: tests={tests} security={security} perf={perf} docs={docs}"),
    }
}

pub(crate) fn suggest_call(target: &str, reason: &str, priority: &str, params: Value) -> Value {
    json!({
        "action": "call_tool",
        "target": target,
        "reason": reason,
        "priority": priority,
        "validated": true,
        "params": params
    })
}

pub(crate) fn suggest_method(method: &str, reason: &str, priority: &str, params: Value) -> Value {
    json!({
        "action": "call_method",
        "method": method,
        "reason": reason,
        "priority": priority,
        "validated": true,
        "params": params
    })
}

pub(crate) fn warning(code: &str, message: &str, recovery: &str) -> Value {
    json!({
        "code": code,
        "message": message,
        "recovery": recovery
    })
}

fn budget_warning_truncated() -> Value {
    warning(
        "BUDGET_TRUNCATED",
        "Response truncated to fit max_chars",
        "Increase max_chars or reduce limit parameters to receive more detail.",
    )
}

fn budget_warning_minimal() -> Value {
    warning(
        "BUDGET_MINIMAL",
        "Response reduced to minimal signal",
        "Increase max_chars or narrow filters/limits to recover full context.",
    )
}

fn budget_warning_clamped() -> Value {
    warning(
        "BUDGET_MIN_CLAMPED",
        "max_chars below minimum; clamped to minimal safe value",
        "Increase max_chars to avoid clamping and receive a fuller payload.",
    )
}

pub(crate) fn budget_warnings(truncated: bool, minimal: bool, clamped: bool) -> Vec<Value> {
    let mut out = Vec::new();
    if truncated {
        out.push(budget_warning_truncated());
    }
    if minimal {
        out.push(budget_warning_minimal());
    }
    if clamped {
        out.push(budget_warning_clamped());
    }
    out
}

pub(crate) fn set_truncated_flag(value: &mut Value, truncated: bool) {
    if let Some(obj) = value.as_object_mut()
        && obj.contains_key("truncated")
    {
        obj.insert("truncated".to_string(), Value::Bool(truncated));
    }
}

pub(crate) fn ai_ok_with_warnings(
    intent: &str,
    result: Value,
    warnings: Vec<Value>,
    suggestions: Vec<Value>,
) -> Value {
    json!({
        "success": true,
        "intent": intent,
        "result": result,
        "warnings": warnings,
        "suggestions": suggestions,
        "context": {},
        "error": null,
        "timestamp": now_rfc3339(),
    })
}

pub(crate) fn ai_ok_with(intent: &str, result: Value, suggestions: Vec<Value>) -> Value {
    ai_ok_with_warnings(intent, result, Vec::new(), suggestions)
}

pub(crate) fn ai_error_with(
    code: &str,
    message: &str,
    recovery: Option<&str>,
    suggestions: Vec<Value>,
) -> Value {
    // UX invariant: keep `message` and `recovery` semantically separated.
    // If we already provide a dedicated recovery string, avoid embedding another `fix:` hint
    // into the message, otherwise line-protocol renders a noisy double-fix.
    let message = if code == "INVALID_INPUT" && recovery.is_none() {
        enrich_invalid_input_message(message)
    } else {
        message.to_string()
    };
    let error = match recovery {
        None => json!({ "code": code, "message": message }),
        Some(recovery) => json!({ "code": code, "message": message, "recovery": recovery }),
    };
    json!({
        "success": false,
        "intent": "error",
        "result": {},
        "warnings": [],
        "suggestions": suggestions,
        "context": {},
        "error": error,
        "timestamp": now_rfc3339(),
    })
}

pub(crate) fn ai_ok(intent: &str, result: Value) -> Value {
    ai_ok_with(intent, result, Vec::new())
}

pub(crate) fn ai_error(code: &str, message: &str) -> Value {
    ai_error_with(code, message, None, Vec::new())
}

fn enrich_invalid_input_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.contains("fix:") || trimmed.contains("expected ") {
        return trimmed.to_string();
    }

    if trimmed == "arguments must be an object" {
        return "arguments: expected object; fix: {\"workspace\":\"ws\"}".to_string();
    }

    if let Some(field) = trimmed.strip_suffix(" is required") {
        let field = field.trim();
        return format!("{field}: expected required value; fix: {field}=\"...\"");
    }

    if let Some(field) = trimmed.strip_suffix(" must not be empty") {
        let field = field.trim();
        return format!("{field}: expected non-empty value; fix: {field}=\"...\"");
    }

    if let Some(field) = trimmed.strip_suffix(" must be an object") {
        let field = field.trim();
        return format!("{field}: expected object; fix: {field}={{...}}");
    }

    if let Some(field) = trimmed.strip_suffix(" must be an array") {
        let field = field.trim();
        return format!("{field}: expected array; fix: {field}=[{{...}}]");
    }

    if let Some(field) = trimmed.strip_suffix(" must be an array of strings") {
        let field = field.trim();
        return format!("{field}: expected array of strings; fix: {field}=[\"...\"]");
    }

    if let Some(field) = trimmed.strip_suffix(" must be a string") {
        let field = field.trim();
        return format!("{field}: expected string; fix: {field}=\"...\"");
    }

    if let Some(field) = trimmed.strip_suffix(" must be an integer") {
        let field = field.trim();
        return format!("{field}: expected integer; fix: {field}=1");
    }

    if let Some(field) = trimmed.strip_suffix(" must be a positive integer") {
        let field = field.trim();
        return format!("{field}: expected positive integer; fix: {field}=1");
    }

    if let Some(field) = trimmed.strip_suffix(" must be a boolean") {
        let field = field.trim();
        return format!("{field}: expected boolean; fix: {field}=true");
    }

    if trimmed.contains("must start with PLAN- or TASK-") {
        let field = trimmed.split_whitespace().next().unwrap_or("target");
        return format!("{field}: expected PLAN- or TASK- id; fix: {field}=\"TASK-001\"");
    }

    if trimmed.contains("provide") && trimmed.contains("not both") {
        return format!("{trimmed}; fix: choose one option (e.g., task=\"TASK-001\")");
    }

    // Default: do not add generic wrapper noise — surface the real message.
    // Tool implementations and the schema layer already provide concrete "fix" hints for
    // common input mistakes.
    trimmed.to_string()
}
