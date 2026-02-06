#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::override_apply::{apply_override_from_ctx, build_override_suggestion};
use super::spec::{GateSpec, status_is_closed};

pub(super) fn enforce_deep_synthesis(
    ctx: &mut super::ReasoningGateContext<'_>,
    spec: &GateSpec,
    step_ref: &bm_storage::StepRef,
    _reasoning_ref: &bm_storage::ReasoningRefRow,
    cards: &[Value],
) -> Result<(), Value> {
    let hypotheses_total = cards
        .iter()
        .filter(|c| c.get("type").and_then(|v| v.as_str()) == Some("hypothesis"))
        .count();
    let has_resolved_decision = cards.iter().any(|card| {
        if card.get("type").and_then(|v| v.as_str()) != Some("decision") {
            return false;
        }
        let status = card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            .trim();
        status_is_closed(status)
    });

    let mut missing = Vec::<String>::new();
    if hypotheses_total < 2 {
        missing.push("DEEP_MIN_2_HYPOTHESES".to_string());
    }
    if !has_resolved_decision {
        missing.push("DEEP_NEEDS_RESOLVED_DECISION".to_string());
    }
    if missing.is_empty() {
        return Ok(());
    }
    let message = format!(
        "deep reasoning: missing required synthesis artifacts: {}",
        missing.join(", ")
    );

    let mut suggestions = vec![suggest_call(
        "think_playbook",
        spec.playbook_hint,
        "medium",
        json!({ "workspace": ctx.workspace.as_str(), "name": spec.playbook_name, "max_chars": 1200 }),
    )];

    if hypotheses_total < 2 {
        suggestions.push(suggest_call(
            "think_card",
            "Add a second hypothesis branch (step-scoped).",
            "high",
            json!({
                "workspace": ctx.workspace.as_str(),
                "target": ctx.task_id.to_string(),
                "step": step_ref.step_id.clone(),
                "card": {
                    "type": "hypothesis",
                    "title": "Hypothesis(alt):TBD",
                    "text": "Write the best alternative hypothesis; add one disconfirming test idea.",
                    "status": "open",
                    "tags": [spec.gate_tag, "branch"]
                }
            }),
        ));
    }
    if !has_resolved_decision {
        suggestions.push(suggest_call(
            "think_card",
            "Record a resolved synthesis decision for this step (required in deep mode).",
            "high",
            json!({
                "workspace": ctx.workspace.as_str(),
                "target": ctx.task_id.to_string(),
                "step": step_ref.step_id.clone(),
                "card": {
                    "type": "decision",
                    "title": "Decision:TBD",
                    "text": "Synthesis: winner + tradeoffs + rollback/stop rule + what would change your mind.",
                    "status": "resolved",
                    "tags": [spec.gate_tag]
                }
            }),
        ));
    }

    if ctx.allow_override
        && let Some(override_input) = ctx.reasoning_override
    {
        apply_override_from_ctx(ctx, override_input, spec.mode_label, missing)?;
        return Ok(());
    }

    suggestions.push(build_override_suggestion(
        ctx.args_obj,
        ctx.workspace.as_str(),
        ctx.task_id,
        step_ref,
        spec.mode_label,
        "Override deep reasoning gate: close step now to unblock; will backfill decision after.",
        "Risk: missing synthesis decision; could lose key tradeoffs and rollback conditions.",
    ));

    Err(ai_error_with(
        "REASONING_REQUIRED",
        &message,
        Some(
            "In deep mode: add 2+ hypotheses and a resolved decision before closing the step, then retry.",
        ),
        suggestions,
    ))
}
