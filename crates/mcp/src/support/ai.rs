#![forbid(unsafe_code)]

use crate::support::time::now_rfc3339;
use bm_storage::StoreError;
use serde_json::{Value, json};

pub(crate) fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::ProjectGuardMismatch { expected, stored } => {
            format!("Project guard mismatch: expected={expected} stored={stored}")
        }
        StoreError::RevisionMismatch { expected, actual } => {
            format!("Revision mismatch: expected={expected} actual={actual}")
        }
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::JobNotClaimable { job_id, status } => {
            format!("Job not claimable: job_id={job_id} status={status}")
        }
        StoreError::JobNotRunning { job_id, status } => {
            format!("Job not running: job_id={job_id} status={status}")
        }
        StoreError::JobNotCancelable { job_id, status } => {
            format!("Job not cancelable: job_id={job_id} status={status}")
        }
        StoreError::JobClaimMismatch {
            job_id,
            expected_runner_id,
            actual_runner_id,
            expected_revision,
            actual_revision,
        } => match expected_runner_id {
            Some(expected_runner_id) => format!(
                "Job claim mismatch: job_id={job_id} expected_runner_id={expected_runner_id} actual_runner_id={actual_runner_id} expected_revision={expected_revision} actual_revision={actual_revision}"
            ),
            None => format!(
                "Job claim mismatch: job_id={job_id} expected_runner_id=<none> actual_runner_id={actual_runner_id} expected_revision={expected_revision} actual_revision={actual_revision}"
            ),
        },
        StoreError::JobNotMessageable { job_id, status } => {
            format!("Job not messageable: job_id={job_id} status={status}")
        }
        StoreError::JobAlreadyTerminal { job_id, status } => {
            format!("Job already terminal: job_id={job_id} status={status}")
        }
        StoreError::JobNotRequeueable { job_id, status } => {
            format!("Job not requeueable: job_id={job_id} status={status}")
        }
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
        StoreError::StepLeaseHeld {
            step_id,
            holder_agent_id,
            now_seq,
            expires_seq,
        } => format!(
            "Step lease held: step_id={step_id} holder={holder_agent_id} now_seq={now_seq} expires_seq={expires_seq}"
        ),
        StoreError::StepLeaseNotHeld {
            step_id,
            holder_agent_id,
        } => match holder_agent_id {
            None => format!("Step lease not held: step_id={step_id} (no active lease)"),
            Some(holder) => format!("Step lease not held: step_id={step_id} holder={holder}"),
        },
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
    let raw_message = message.trim();
    let hints = if code == "INVALID_INPUT" {
        invalid_input_hints(raw_message)
    } else {
        Vec::new()
    };

    // UX invariant: keep `message` and `recovery` semantically separated.
    // If we already provide a dedicated recovery string, avoid embedding another `fix:` hint
    // into the message, otherwise line-protocol renders a noisy double-fix.
    let message = if code == "INVALID_INPUT" && recovery.is_none() {
        enrich_invalid_input_message(raw_message)
    } else {
        raw_message.to_string()
    };

    let mut error_obj = serde_json::Map::new();
    error_obj.insert("code".to_string(), Value::String(code.to_string()));
    error_obj.insert("message".to_string(), Value::String(message));
    if let Some(recovery) = recovery {
        error_obj.insert("recovery".to_string(), Value::String(recovery.to_string()));
    }
    if code == "INVALID_INPUT" && !hints.is_empty() {
        error_obj.insert("hints".to_string(), Value::Array(hints));
    }
    let error = Value::Object(error_obj);

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

    // Graph node ids are used across the system (cards, graph edges, etc). When a user/agent
    // accidentally pastes large text into an id field, the raw storage error is hard to act on.
    // Provide a copy/paste-friendly hint that points back to stable ids.
    if trimmed == "node id is too long" {
        return "id: too long (max 1024 chars); fix: use a stable short id (CARD-... / TASK-... / PLAN-... / JOB-... / a:<anchor>)".to_string();
    }
    if trimmed == "node id must not be empty" {
        return "id: expected non-empty; fix: use CARD-... / TASK-... / PLAN-... / JOB-... / a:<anchor>".to_string();
    }
    if trimmed == "node id must not contain '|'" {
        return "id: must not contain '|'; fix: use a stable short id (CARD-... / TASK-... / ... )"
            .to_string();
    }
    if trimmed == "node id contains control characters" {
        return "id: contains control characters; fix: copy the id again (CARD-... / TASK-... / ...)".to_string();
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

    if let Some((a, b)) = parse_mutually_exclusive_fields(trimmed) {
        return format!("{trimmed}; fix: omit {a} OR omit {b}");
    }

    // Default: do not add generic wrapper noise — surface the real message.
    // Tool implementations and the schema layer already provide concrete "fix" hints for
    // common input mistakes.
    trimmed.to_string()
}

fn parse_mutually_exclusive_fields(message: &str) -> Option<(String, String)> {
    let trimmed = message.trim();
    if !trimmed.starts_with("provide ") || !trimmed.ends_with(", not both") {
        return None;
    }
    let fields_part = trimmed
        .strip_prefix("provide ")
        .and_then(|s| s.strip_suffix(", not both"))?
        .trim();

    let mut parts = fields_part.split(" or ");
    let a = parts.next()?.trim();
    let b = parts.next()?.trim();
    if parts.next().is_some() || a.is_empty() || b.is_empty() {
        return None;
    }
    Some((a.to_string(), b.to_string()))
}

fn invalid_input_hints(message: &str) -> Vec<Value> {
    let trimmed = message.trim();
    let mut hints = Vec::new();

    if trimmed == "arguments must be an object" {
        hints.push(json!({
            "kind": "type",
            "field": "arguments",
            "expected": "object"
        }));
        return hints;
    }

    if trimmed == "node id is too long" {
        hints.push(json!({
            "kind": "length",
            "subject": "graph_node_id",
            "max_chars": 1024
        }));
        return hints;
    }

    if let Some((a, b)) = parse_mutually_exclusive_fields(trimmed) {
        hints.push(json!({
            "kind": "choose_one",
            "fields": [a.clone(), b.clone()],
            "options": [
                { "keep": [a.clone()], "drop": [b.clone()] },
                { "keep": [b], "drop": [a] }
            ]
        }));
    }

    if let Some(field) = trimmed.strip_suffix(" is required") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "missing_required",
                "field": field
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must not be empty") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "non_empty",
                "field": field
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be an object") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "object"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be an array") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "array"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be an array of strings") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "array",
                "items": "string"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be a string") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "string"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be an integer") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "integer"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be a positive integer") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "positive_integer"
            }));
        }
    }

    if let Some(field) = trimmed.strip_suffix(" must be a boolean") {
        let field = field.trim();
        if !field.is_empty() {
            hints.push(json!({
                "kind": "type",
                "field": field,
                "expected": "boolean"
            }));
        }
    }

    if trimmed.contains("must start with PLAN- or TASK-") {
        let field = trimmed.split_whitespace().next().unwrap_or("target");
        hints.push(json!({
            "kind": "prefix",
            "field": field,
            "allowed": ["PLAN-", "TASK-"]
        }));
    } else if trimmed.contains("must start with PLAN-") {
        let field = trimmed.split_whitespace().next().unwrap_or("plan");
        hints.push(json!({
            "kind": "prefix",
            "field": field,
            "allowed": ["PLAN-"]
        }));
    } else if trimmed.contains("must start with TASK-") {
        let field = trimmed.split_whitespace().next().unwrap_or("task");
        hints.push(json!({
            "kind": "prefix",
            "field": field,
            "allowed": ["TASK-"]
        }));
    }

    hints
}
