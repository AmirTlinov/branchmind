#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::override_apply::{apply_override_from_ctx, build_override_suggestion};
use super::spec::GateSpec;

pub(super) fn enforce_discipline_signals(
    ctx: &mut super::ReasoningGateContext<'_>,
    spec: &GateSpec,
    step_ref: &bm_storage::StepRef,
    reasoning_ref: &bm_storage::ReasoningRefRow,
    step_tag: &str,
    cards: &[Value],
    edges: &[Value],
) -> Result<(), Value> {
    let trace_entries = match ctx.server.store.doc_show_tail(
        ctx.workspace,
        &reasoning_ref.branch,
        &reasoning_ref.trace_doc,
        None,
        80,
    ) {
        Ok(slice) => doc_entries_to_json(slice.entries),
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let engine = derive_reasoning_engine_step_aware(
        EngineScope {
            workspace: ctx.workspace.as_str(),
            branch: reasoning_ref.branch.as_str(),
            graph_doc: reasoning_ref.graph_doc.as_str(),
            trace_doc: reasoning_ref.trace_doc.as_str(),
        },
        cards,
        edges,
        &trace_entries,
        Some(step_tag),
        EngineLimits {
            signals_limit: 200,
            actions_limit: 0,
        },
    );
    let engine = match engine {
        Some(v) => v,
        None => {
            if ctx.allow_override
                && let Some(override_input) = ctx.reasoning_override
            {
                apply_override_from_ctx(
                    ctx,
                    override_input,
                    spec.mode_label,
                    vec!["GATE_ENGINE_NO_SIGNALS".to_string()],
                )?;
                return Ok(());
            }

            let mut suggestions = vec![suggest_call(
                "think_playbook",
                spec.playbook_hint,
                "medium",
                json!({ "workspace": ctx.workspace.as_str(), "name": spec.playbook_name, "max_chars": 1200 }),
            )];
            suggestions.push(build_override_suggestion(
                ctx.args_obj,
                ctx.workspace.as_str(),
                ctx.task_id,
                step_ref,
                spec.mode_label,
                "Override reasoning gate: close step now to unblock; will backfill reasoning artifacts after.",
                "Risk: reasoning engine produced no signals; could mean weak evidence or missing structure.",
            ));
            return Err(ai_error_with(
                "REASONING_REQUIRED",
                &format!(
                    "{} reasoning: reasoning engine produced no signals/actions",
                    spec.mode_label
                ),
                Some("Seed a minimal hypothesis+test for this step, then retry."),
                suggestions,
            ));
        }
    };

    let shorten = |s: &str, max: usize| s.chars().take(max).collect::<String>();
    let card_label = |id: &str, cards: &[Value]| {
        cards
            .iter()
            .find(|c| c.get("id").and_then(|v| v.as_str()) == Some(id))
            .and_then(|c| c.get("title").and_then(|v| v.as_str()))
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| shorten(t, 64))
            .unwrap_or_else(|| id.to_string())
    };

    let mut missing = Vec::<String>::new();
    let mut suggestions = vec![suggest_call(
        "think_playbook",
        spec.playbook_hint,
        "medium",
        json!({ "workspace": ctx.workspace.as_str(), "name": spec.playbook_name, "max_chars": 1200 }),
    )];

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for signal in signals {
        let code = signal.get("code").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(code, "BM4_HYPOTHESIS_NO_TEST" | "BM10_NO_COUNTER_EDGES") {
            continue;
        }
        if !missing.contains(&code.to_string()) {
            missing.push(code.to_string());
        }
        let target_id = signal
            .get("refs")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str());
        let Some(target_id) = target_id else {
            continue;
        };
        let label = card_label(target_id, cards);

        if code == "BM4_HYPOTHESIS_NO_TEST" && suggestions.len() < 3 {
            suggestions.push(suggest_call(
                "think_card",
                "Add a test stub for this hypothesis (step-scoped).",
                "high",
                json!({
                    "workspace": ctx.workspace.as_str(),
                    "target": ctx.task_id.to_string(),
                    "step": step_ref.step_id.clone(),
                    "card": {
                        "type": "test",
                        "title": format!("Test: {label}"),
                        "text": "Define the smallest runnable check for this hypothesis.",
                        "status": "open",
                        "tags": ["bm4"]
                    },
                    "supports": [target_id]
                }),
            ));
        }
        if code == "BM10_NO_COUNTER_EDGES" && suggestions.len() < 3 {
            suggestions.push(suggest_call(
                "think_card",
                "Steelman a counter-hypothesis (step-scoped).",
                "high",
                json!({
                    "workspace": ctx.workspace.as_str(),
                    "target": ctx.task_id.to_string(),
                    "step": step_ref.step_id.clone(),
                    "card": {
                        "type": "hypothesis",
                        "title": format!("Counter-hypothesis: {label}"),
                        "text": "Steelman the opposite case; include 1 disconfirming test idea.",
                        "status": "open",
                        "tags": ["bm7", "counter"]
                    },
                    "blocks": [target_id]
                }),
            ));
        }
    }

    if missing.is_empty() {
        return Ok(());
    }

    let message = format!(
        "{} reasoning: missing discipline signals: {}",
        spec.mode_label,
        missing.join(", ")
    );
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
        "Override reasoning gate: close step now to unblock; will backfill tests/counter-case after.",
        "Risk: missing strict discipline signals; could hide flaws.",
    ));
    Err(ai_error_with(
        "REASONING_REQUIRED",
        &message,
        Some(
            "Fix the missing reasoning artifacts (tests + counter-position) for this step, then retry.",
        ),
        suggestions,
    ))
}
