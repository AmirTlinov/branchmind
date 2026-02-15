#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

const STRICT_SEQUENTIAL_MIN_STEPS: usize = 2;
const STRICT_SEQUENTIAL_SCAN_LIMIT: usize = 200;

#[derive(Clone, Debug)]
pub(crate) struct StrictReasoningOverride {
    reason: String,
    risk: String,
}

pub(crate) fn parse_strict_reasoning_override(
    value: Option<&Value>,
) -> Result<Option<StrictReasoningOverride>, Value> {
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

    Ok(Some(StrictReasoningOverride { reason, risk }))
}

pub(crate) struct StrictGateContext<'a> {
    pub(crate) server: &'a mut McpServer,
    pub(crate) workspace: &'a WorkspaceId,
    pub(crate) task_id: &'a str,
    pub(crate) step_id: Option<&'a str>,
    pub(crate) path: Option<&'a StepPath>,
    pub(crate) args_obj: &'a serde_json::Map<String, Value>,
    pub(crate) reasoning_override: Option<&'a StrictReasoningOverride>,
    pub(crate) allow_override: bool,
    pub(crate) close_args_obj: Option<&'a mut serde_json::Map<String, Value>>,
    pub(crate) warnings: Option<&'a mut Vec<Value>>,
    pub(crate) note_event: Option<&'a mut Option<Value>>,
}

pub(crate) fn enforce_strict_reasoning_gate(mut ctx: StrictGateContext<'_>) -> Result<bool, Value> {
    let strict = match ctx.server.store.get_task(ctx.workspace, ctx.task_id) {
        Ok(Some(task)) => task.reasoning_mode == "strict",
        Ok(None) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    if !strict {
        return Ok(false);
    }

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

    let status_is_closed_for_strict = |status: &str| {
        status.eq_ignore_ascii_case("closed")
            || status.eq_ignore_ascii_case("done")
            || status.eq_ignore_ascii_case("resolved")
    };
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
        !status_is_closed_for_strict(status)
    });

    let mut strict_overridden = false;
    if !has_active_hypothesis_or_decision {
        let mut params = serde_json::Map::new();
        params.insert(
            "workspace".to_string(),
            Value::String(ctx.workspace.as_str().to_string()),
        );
        params.insert("target".to_string(), Value::String(ctx.task_id.to_string()));
        params.insert(
            "branch".to_string(),
            Value::String(reasoning_ref.branch.clone()),
        );
        params.insert(
            "trace_doc".to_string(),
            Value::String(reasoning_ref.trace_doc.clone()),
        );
        params.insert(
            "graph_doc".to_string(),
            Value::String(reasoning_ref.graph_doc.clone()),
        );
        params.insert("step".to_string(), Value::String(step_ref.step_id.clone()));
        params.insert(
            "card".to_string(),
            json!({
                "type": "hypothesis",
                "title": "Hypothesis: <fill>",
                "text": "State the simplest falsifiable hypothesis for this step, then link a test.",
                "status": "open",
                "tags": ["bm-strict"]
            }),
        );

        let mut suggestions = vec![
            suggest_call(
                "think_playbook",
                "Load strict reasoning playbook (skepticism checklist).",
                "medium",
                json!({ "workspace": ctx.workspace.as_str(), "name": "strict", "max_chars": 1200 }),
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
            apply_strict_override_from_ctx(
                &mut ctx,
                override_input,
                vec!["STRICT_NO_HYPOTHESIS_OR_DECISION".to_string()],
            )?;
            strict_overridden = true;
        } else {
            suggestions.push(build_override_suggestion(
                ctx.args_obj,
                ctx.workspace.as_str(),
                ctx.task_id,
                &step_ref,
                "Override strict gate: closing step now to unblock; will backfill hypothesis/test.",
                "Risk: reduced confidence; missing falsifier/counter-case could hide flaws.",
            ));
            return Err(ai_error_with(
                "REASONING_REQUIRED",
                "strict reasoning: step requires at least one hypothesis/decision",
                Some(
                    "Add a step-scoped hypothesis (and a test) before closing the step, then retry.",
                ),
                suggestions,
            ));
        }
    }

    if !strict_overridden {
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
            &cards,
            &edges,
            &trace_entries,
            Some(step_tag.as_str()),
            EngineLimits {
                signals_limit: 200,
                actions_limit: 0,
            },
        );
        let engine = match engine {
            Some(v) => Some(v),
            None => {
                if ctx.allow_override
                    && let Some(override_input) = ctx.reasoning_override
                {
                    apply_strict_override_from_ctx(
                        &mut ctx,
                        override_input,
                        vec!["STRICT_ENGINE_NO_SIGNALS".to_string()],
                    )?;
                    None
                } else {
                    let mut suggestions = vec![suggest_call(
                        "think_playbook",
                        "Load strict reasoning playbook (skepticism checklist).",
                        "medium",
                        json!({ "workspace": ctx.workspace.as_str(), "name": "strict", "max_chars": 1200 }),
                    )];
                    suggestions.push(build_override_suggestion(
                        ctx.args_obj,
                        ctx.workspace.as_str(),
                        ctx.task_id,
                        &step_ref,
                        "Override strict gate: closing step now to unblock; will backfill reasoning artifacts after.",
                        "Risk: strict engine produced no signals; could mean weak evidence or missing structure.",
                    ));
                    return Err(ai_error_with(
                        "REASONING_REQUIRED",
                        "strict reasoning: reasoning engine produced no signals/actions",
                        Some("Seed a minimal hypothesis+test for this step, then retry."),
                        suggestions,
                    ));
                }
            }
        };

        if let Some(engine) = engine {
            let shorten = |s: &str, max: usize| s.chars().take(max).collect::<String>();
            let card_label = |id: &str| {
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
                "Load strict reasoning playbook (skepticism checklist).",
                "medium",
                json!({ "workspace": ctx.workspace.as_str(), "name": "strict", "max_chars": 1200 }),
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
                let label = card_label(target_id);

                if code == "BM4_HYPOTHESIS_NO_TEST" && suggestions.len() < 3 {
                    suggestions.push(suggest_call(
                        "think_card",
                        "Add a test stub for this hypothesis (step-scoped).",
                        "high",
                        json!({
                            "workspace": ctx.workspace.as_str(),
                            "target": ctx.task_id.to_string(),
                            "branch": reasoning_ref.branch.clone(),
                            "trace_doc": reasoning_ref.trace_doc.clone(),
                            "graph_doc": reasoning_ref.graph_doc.clone(),
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
                        "think_macro_counter_hypothesis_stub",
                        "Create a counter-hypothesis + test stub (step-scoped; prevents counter→counter regress).",
                        "high",
                        json!({
                            "workspace": ctx.workspace.as_str(),
                            "target": ctx.task_id.to_string(),
                            "branch": reasoning_ref.branch.clone(),
                            "trace_doc": reasoning_ref.trace_doc.clone(),
                            "graph_doc": reasoning_ref.graph_doc.clone(),
                            "step": step_ref.step_id.clone(),
                            "against": target_id,
                            "label": label
                        }),
                    ));
                }
            }

            if !missing.is_empty() {
                let message = format!(
                    "strict reasoning: missing discipline signals: {}",
                    missing.join(", ")
                );
                if ctx.allow_override
                    && let Some(override_input) = ctx.reasoning_override
                {
                    apply_strict_override_from_ctx(&mut ctx, override_input, missing)?;
                } else {
                    let base_recovery = "Fix the missing reasoning artifacts (tests + counter-position) for this step, then retry.";
                    let recovery = if missing
                        .iter()
                        .any(|code| code.trim() == "BM10_NO_COUNTER_EDGES")
                    {
                        format!(
                            "{base_recovery} Note: if you add a counter-hypothesis manually, include tag `counter` (otherwise BM10 may ask for a counter-position for the counter-hypothesis too)."
                        )
                    } else {
                        base_recovery.to_string()
                    };
                    suggestions.push(build_override_suggestion(
                        ctx.args_obj,
                        ctx.workspace.as_str(),
                        ctx.task_id,
                        &step_ref,
                        "Override strict gate: closing step now to unblock; will backfill tests/counter-case after.",
                        "Risk: missing strict discipline signals; could hide flaws.",
                    ));
                    return Err(ai_error_with(
                        "REASONING_REQUIRED",
                        &message,
                        Some(recovery.as_str()),
                        suggestions,
                    ));
                }
            }
        }
    }

    if !strict_overridden && checkpoints_gate_requested(ctx.args_obj) {
        let seq_override =
            enforce_sequential_trace_gate(&mut ctx, &reasoning_ref, &step_ref, &step_tag)?;
        if seq_override {
            strict_overridden = true;
        }
    }

    Ok(strict_overridden)
}

fn build_override_suggestion(
    args_obj: &serde_json::Map<String, Value>,
    workspace: &str,
    task_id: &str,
    step_ref: &bm_storage::StepRef,
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
        "Override strict gate with reason+risk (escape hatch; leaves an explicit debt note).",
        "low",
        Value::Object(override_params),
    )
}

fn render_strict_override_note(reason: &str, risk: &str, missing: &[String]) -> String {
    let mut out = String::new();
    out.push_str("STRICT OVERRIDE (reasoning_mode=strict)\n");
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

fn apply_strict_override_from_ctx(
    ctx: &mut StrictGateContext<'_>,
    input: &StrictReasoningOverride,
    missing: Vec<String>,
) -> Result<(), Value> {
    let (Some(close_args_obj), Some(warnings), Some(note_event)) = (
        ctx.close_args_obj.as_deref_mut(),
        ctx.warnings.as_deref_mut(),
        ctx.note_event.as_deref_mut(),
    ) else {
        return Err(ai_error(
            "INVALID_INPUT",
            "strict override requires mutable close context",
        ));
    };
    apply_strict_override(
        ctx.server,
        close_args_obj,
        warnings,
        note_event,
        input,
        missing,
    )
}

fn apply_strict_override(
    server: &mut McpServer,
    close_args_obj: &mut serde_json::Map<String, Value>,
    warnings: &mut Vec<Value>,
    note_event: &mut Option<Value>,
    input: &StrictReasoningOverride,
    missing: Vec<String>,
) -> Result<(), Value> {
    let note_text = render_strict_override_note(&input.reason, &input.risk, &missing);

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
        "STRICT_OVERRIDE_APPLIED",
        &format!("strict gate overridden; missing={missing_label}"),
        "A reason+risk note was recorded. Treat this as temporary debt; schedule a follow-up to validate.",
    ));

    Ok(())
}

fn checkpoints_gate_requested(args_obj: &serde_json::Map<String, Value>) -> bool {
    match args_obj.get("checkpoints") {
        Some(Value::String(mode)) => mode.trim().eq_ignore_ascii_case("gate"),
        Some(Value::Object(obj)) => {
            let all = obj.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
            if all {
                return true;
            }
            let criteria = obj
                .get("criteria")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let tests = obj.get("tests").and_then(|v| v.as_bool()).unwrap_or(false);
            criteria && tests
        }
        _ => false,
    }
}

fn enforce_sequential_trace_gate(
    ctx: &mut StrictGateContext<'_>,
    reasoning_ref: &bm_storage::ReasoningRefRow,
    step_ref: &bm_storage::StepRef,
    step_tag: &str,
) -> Result<bool, Value> {
    let trace_tail = match ctx.server.store.doc_show_tail(
        ctx.workspace,
        &reasoning_ref.branch,
        &reasoning_ref.trace_doc,
        None,
        STRICT_SEQUENTIAL_SCAN_LIMIT,
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let mut total_seq = 0usize;
    let mut step_seq = 0usize;
    for entry in trace_tail.entries {
        if entry.format.as_deref() != Some("trace_sequential_step") {
            continue;
        }
        total_seq = total_seq.saturating_add(1);
        if sequential_entry_matches_step(&entry, &step_ref.step_id, step_tag) {
            step_seq = step_seq.saturating_add(1);
        }
    }
    let has_enough = step_seq >= STRICT_SEQUENTIAL_MIN_STEPS
        || (step_seq == 0 && total_seq >= STRICT_SEQUENTIAL_MIN_STEPS);
    if has_enough {
        return Ok(false);
    }

    if ctx.allow_override
        && let Some(override_input) = ctx.reasoning_override
    {
        apply_strict_override_from_ctx(
            ctx,
            override_input,
            vec!["STRICT_NO_SEQUENTIAL_TRACE".to_string()],
        )?;
        return Ok(true);
    }

    let next_num = step_seq.saturating_add(1);
    let mut suggestions = vec![suggest_call(
        "think_playbook",
        "Load strict reasoning playbook (structured sequential checkpoints).",
        "medium",
        json!({ "workspace": ctx.workspace.as_str(), "name": "strict", "max_chars": 1200 }),
    )];
    suggestions.push(suggest_call(
        "think_trace_sequential_step",
        "Add a structured sequential trace checkpoint for this step.",
        "high",
        json!({
            "workspace": ctx.workspace.as_str(),
            "target": ctx.task_id.to_string(),
            "branch": reasoning_ref.branch.clone(),
            "doc": reasoning_ref.trace_doc.clone(),
            "thought": "Checkpoint: hypothesis/test/counter status for this step.",
            "thoughtNumber": next_num,
            "totalThoughts": STRICT_SEQUENTIAL_MIN_STEPS,
            "nextThoughtNeeded": next_num < STRICT_SEQUENTIAL_MIN_STEPS,
            "message": format!("sequential gate for {}", step_ref.step_id),
            "meta": {
                "step_id": step_ref.step_id.clone(),
                "step_tag": step_tag,
                "checkpoint": "gate",
                "structured": true
            }
        }),
    ));
    suggestions.push(build_override_suggestion(
        ctx.args_obj,
        ctx.workspace.as_str(),
        ctx.task_id,
        step_ref,
        "Override sequential gate: close now and backfill trace checkpoints.",
        "Risk: close without explicit sequential gate trail may hide reasoning gaps.",
    ));

    Err(ai_error_with(
        "REASONING_REQUIRED",
        &format!(
            "strict reasoning: checkpoints=gate requires at least {STRICT_SEQUENTIAL_MIN_STEPS} sequential trace steps (step_scoped={step_seq}, total={total_seq})"
        ),
        Some(
            "Add structured think.trace.sequential.step checkpoints for this step, then retry close.",
        ),
        suggestions,
    ))
}

fn sequential_entry_matches_step(
    entry: &bm_storage::DocEntryRow,
    step_id: &str,
    step_tag: &str,
) -> bool {
    let Some(raw) = entry.meta_json.as_deref() else {
        return false;
    };
    let Ok(meta_value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    let Some(meta) = meta_value.as_object() else {
        return false;
    };

    let step_tag_lc = step_tag.to_ascii_lowercase();
    let step_id_lc = step_id.to_ascii_lowercase();
    let key_matches = |value: &str| {
        let val = value.trim().to_ascii_lowercase();
        !val.is_empty() && (val == step_id_lc || val == step_tag_lc)
    };

    if meta
        .get("step_id")
        .and_then(|v| v.as_str())
        .is_some_and(key_matches)
    {
        return true;
    }
    if meta
        .get("step_tag")
        .and_then(|v| v.as_str())
        .is_some_and(key_matches)
    {
        return true;
    }
    if meta
        .get("step")
        .and_then(|v| v.as_str())
        .is_some_and(key_matches)
    {
        return true;
    }
    meta.get("tags")
        .and_then(|v| v.as_array())
        .is_some_and(|arr| arr.iter().filter_map(|v| v.as_str()).any(key_matches))
}
