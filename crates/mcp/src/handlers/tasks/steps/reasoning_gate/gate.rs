#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::override_apply::{apply_override_from_ctx, build_override_suggestion};
use super::spec::{GateMode, GateSpec, status_is_closed};

pub(crate) fn enforce_reasoning_gate(
    mut ctx: super::ReasoningGateContext<'_>,
) -> Result<bool, Value> {
    let reasoning_mode = match ctx.server.store.get_task(ctx.workspace, ctx.task_id) {
        Ok(Some(task)) => task.reasoning_mode.trim().to_ascii_lowercase(),
        Ok(None) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(spec) = GateSpec::for_task_reasoning_mode(&reasoning_mode) else {
        return Ok(false);
    };

    let step_ref =
        match ctx
            .server
            .store
            .step_resolve(ctx.workspace, ctx.task_id, ctx.step_id, ctx.path)
        {
            Ok(v) => v,
            Err(StoreError::StepNotFound) => return Err(ai_error("UNKNOWN_ID", "Step not found")),
            Err(StoreError::UnknownId) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
    let step_tag = step_tag_for(&step_ref.step_id);

    let (reasoning_ref, _reasoning_exists) = match resolve_reasoning_ref_for_read(
        &mut ctx.server.store,
        ctx.workspace,
        ctx.task_id,
        TaskKind::Task,
        false,
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let types = bm_core::think::SUPPORTED_THINK_CARD_TYPES
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    let (cards, edges) = match ctx.server.store.graph_query(
        ctx.workspace,
        &reasoning_ref.branch,
        &reasoning_ref.graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types),
            status: None,
            tags_any: None,
            tags_all: Some(vec![step_tag.clone()]),
            text: None,
            cursor: None,
            limit: 200,
            include_edges: true,
            edges_limit: 600,
        },
    ) {
        Ok(slice) => (
            graph_nodes_to_cards(slice.nodes),
            graph_edges_to_json(slice.edges),
        ),
        Err(StoreError::UnknownBranch) => (Vec::new(), Vec::new()),
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let mut override_applied = false;

    // Gate-0: require at least one active hypothesis/decision to avoid "status drift" bypasses.
    let has_active_hypothesis_or_decision = cards.iter().any(|card| {
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(ty, "hypothesis" | "decision") {
            return false;
        }
        let status = card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            .trim();
        !status_is_closed(status)
    });

    if !has_active_hypothesis_or_decision {
        let mut params = serde_json::Map::new();
        params.insert(
            "workspace".to_string(),
            Value::String(ctx.workspace.as_str().to_string()),
        );
        params.insert("target".to_string(), Value::String(ctx.task_id.to_string()));
        params.insert("step".to_string(), Value::String(step_ref.step_id.clone()));
        params.insert(
            "card".to_string(),
            json!({
                "type": "hypothesis",
                // NOTE: This payload is rendered into a BM-L1 copy/paste command line.
                // Keep JSON string values whitespace-free so `split_whitespace` parsing stays valid.
                "title": "Hypothesis:TBD",
                "text": "TBD",
                "status": "open",
                "tags": [spec.gate_tag]
            }),
        );

        let mut suggestions = vec![
            suggest_call(
                "think_playbook",
                spec.playbook_hint,
                "medium",
                json!({ "workspace": ctx.workspace.as_str(), "name": spec.playbook_name, "max_chars": 1200 }),
            ),
            suggest_call(
                "think_card",
                "Create a step-scoped hypothesis skeleton.",
                "high",
                Value::Object(params),
            ),
        ];

        if ctx.allow_override
            && let Some(override_input) = ctx.reasoning_override
        {
            apply_override_from_ctx(
                &mut ctx,
                override_input,
                spec.mode_label,
                vec!["GATE_NO_HYPOTHESIS_OR_DECISION".to_string()],
            )?;
            override_applied = true;
        } else {
            suggestions.push(build_override_suggestion(
                ctx.args_obj,
                ctx.workspace.as_str(),
                ctx.task_id,
                &step_ref,
                spec.mode_label,
                "Override reasoning gate: close step now to unblock; will backfill hypothesis/test.",
                "Risk: reduced confidence; missing falsifier/counter-case could hide flaws.",
            ));
            return Err(ai_error_with(
                "REASONING_REQUIRED",
                &format!(
                    "{} reasoning: step requires at least one hypothesis/decision",
                    spec.mode_label
                ),
                Some(
                    "Add a step-scoped hypothesis (and a test) before closing the step, then retry.",
                ),
                suggestions,
            ));
        }
    }

    if !override_applied {
        super::discipline::enforce_discipline_signals(
            &mut ctx,
            &spec,
            &step_ref,
            &reasoning_ref,
            &step_tag,
            &cards,
            &edges,
        )?;
    }

    // Deep is a strict superset: require an explicit synthesis decision before closing.
    if !override_applied && spec.mode == GateMode::Deep {
        super::deep::enforce_deep_synthesis(&mut ctx, &spec, &step_ref, &reasoning_ref, &cards)?;
    }

    Ok(override_applied)
}
