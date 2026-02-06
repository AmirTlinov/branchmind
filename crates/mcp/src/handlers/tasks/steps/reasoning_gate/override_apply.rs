#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct ReasoningOverride {
    reason: String,
    risk: String,
}

pub(crate) fn parse_reasoning_override(
    value: Option<&Value>,
) -> Result<Option<ReasoningOverride>, Value> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(obj) = value.as_object() else {
        return Err(ai_error("INVALID_INPUT", "override must be an object"));
    };

    let reason = match obj.get("reason") {
        None | Some(Value::Null) => {
            return Err(ai_error("INVALID_INPUT", "override.reason is required"));
        }
        Some(Value::String(v)) => {
            let v = v.trim();
            if v.is_empty() {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "override.reason must not be empty",
                ));
            }
            v.to_string()
        }
        Some(_) => {
            return Err(ai_error(
                "INVALID_INPUT",
                "override.reason must be a string",
            ));
        }
    };

    let risk = match obj.get("risk") {
        None | Some(Value::Null) => {
            return Err(ai_error("INVALID_INPUT", "override.risk is required"));
        }
        Some(Value::String(v)) => {
            let v = v.trim();
            if v.is_empty() {
                return Err(ai_error("INVALID_INPUT", "override.risk must not be empty"));
            }
            v.to_string()
        }
        Some(_) => return Err(ai_error("INVALID_INPUT", "override.risk must be a string")),
    };

    Ok(Some(ReasoningOverride { reason, risk }))
}

pub(super) fn build_override_suggestion(
    args_obj: &serde_json::Map<String, Value>,
    workspace: &str,
    task_id: &str,
    step_ref: &bm_storage::StepRef,
    mode_label: &str,
    reason: &str,
    risk: &str,
) -> Value {
    let mut override_params = args_obj.clone();
    override_params.insert(
        "workspace".to_string(),
        Value::String(workspace.to_string()),
    );
    override_params.insert("task".to_string(), Value::String(task_id.to_string()));
    override_params.insert(
        "step_id".to_string(),
        Value::String(step_ref.step_id.clone()),
    );
    override_params.insert("path".to_string(), Value::String(step_ref.path.clone()));
    override_params.insert(
        "override".to_string(),
        json!({
            "reason": reason,
            "risk": risk
        }),
    );
    suggest_call(
        "tasks_macro_close_step",
        &format!(
            "Override {mode_label} reasoning gate with reason+risk (escape hatch; records an explicit debt note)."
        ),
        "low",
        Value::Object(override_params),
    )
}

fn render_override_note(mode_label: &str, reason: &str, risk: &str, missing: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "REASONING OVERRIDE (reasoning_mode={mode_label})\n"
    ));
    out.push_str("Reason: ");
    out.push_str(reason.trim());
    out.push('\n');
    out.push_str("Risk: ");
    out.push_str(risk.trim());
    if !missing.is_empty() {
        out.push('\n');
        out.push_str("Missing: ");
        out.push_str(&missing.join(", "));
    }
    out
}

pub(super) fn apply_override_from_ctx(
    ctx: &mut super::ReasoningGateContext<'_>,
    input: &ReasoningOverride,
    mode_label: &str,
    missing: Vec<String>,
) -> Result<(), Value> {
    let (Some(close_args_obj), Some(warnings), Some(note_event)) = (
        ctx.close_args_obj.as_deref_mut(),
        ctx.warnings.as_deref_mut(),
        ctx.note_event.as_deref_mut(),
    ) else {
        return Err(ai_error(
            "INVALID_INPUT",
            "reasoning override requires mutable close context",
        ));
    };
    apply_override(
        ctx.server,
        close_args_obj,
        warnings,
        note_event,
        input,
        mode_label,
        missing,
    )
}

fn apply_override(
    server: &mut McpServer,
    close_args_obj: &mut serde_json::Map<String, Value>,
    warnings: &mut Vec<Value>,
    note_event: &mut Option<Value>,
    input: &ReasoningOverride,
    mode_label: &str,
    missing: Vec<String>,
) -> Result<(), Value> {
    let note_text = render_override_note(mode_label, &input.reason, &input.risk, &missing);

    let mut note_args = close_args_obj.clone();
    note_args.insert("note".to_string(), Value::String(note_text));
    let note_resp = server.tool_tasks_note(Value::Object(note_args));
    if !note_resp
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(note_resp);
    }
    if let Some(w) = note_resp.get("warnings").and_then(|v| v.as_array()) {
        warnings.extend(w.clone());
    }
    if note_event.is_none() {
        *note_event = note_resp
            .get("result")
            .and_then(|v| v.get("event"))
            .cloned();
    }
    if let Some(revision) = note_resp
        .get("result")
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
    {
        close_args_obj.insert(
            "expected_revision".to_string(),
            Value::Number(serde_json::Number::from(revision)),
        );
    }

    let missing_label = if missing.is_empty() {
        "-".to_string()
    } else {
        missing.join(", ")
    };
    let missing_label = missing_label.chars().take(160).collect::<String>();
    warnings.push(warning(
        "REASONING_OVERRIDE_APPLIED",
        &format!("{mode_label} reasoning gate overridden; missing={missing_label}"),
        "A reason+risk note was recorded. Treat this as temporary debt; schedule a follow-up to validate.",
    ));

    Ok(())
}
