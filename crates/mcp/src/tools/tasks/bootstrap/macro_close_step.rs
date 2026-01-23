#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_macro_close_step(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let resume_max_chars = match optional_usize(args_obj, "resume_max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let view = match optional_string(args_obj, "view") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let note = match optional_string(args_obj, "note") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut proof = args_obj.get("proof").cloned().filter(|v| !v.is_null());
        let reasoning_override = match parse_strict_reasoning_override(args_obj.get("override")) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // Proof UX salvage: when a user/agent pastes receipts into the note (CMD/LINK) but
        // forgets to populate the explicit `proof` field, auto-extract the receipts and attach
        // them as proof checks. This reduces portal friction without changing the proof-required
        // gate semantics (placeholders are still ignored).
        if proof.is_none()
            && let Some(note) = note.as_deref()
        {
            let checks = extract_proof_checks_from_text(note);
            if !checks.is_empty() {
                proof = Some(Value::Array(
                    checks.into_iter().map(Value::String).collect::<Vec<_>>(),
                ));
            }
        }

        let mut warnings = Vec::new();
        let mut note_event: Option<Value> = None;
        let mut evidence_event: Option<Value> = None;
        let mut proof_placeholder_only = false;
        let mut close_args_obj = args_obj.clone();

        let workspace = match require_workspace(&close_args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let has_step_selector =
            close_args_obj.contains_key("step_id") || close_args_obj.contains_key("path");
        if !has_step_selector {
            let (target_id, kind, _focus) =
                match resolve_target_id(&mut self.store, &workspace, args_obj) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            if kind != TaskKind::Task {
                let omit_workspace = self.default_workspace.as_deref() == Some(workspace.as_str());

                let mut start_params = serde_json::Map::new();
                if !omit_workspace {
                    start_params.insert(
                        "workspace".to_string(),
                        Value::String(workspace.as_str().to_string()),
                    );
                }
                start_params.insert("plan".to_string(), Value::String(target_id.clone()));
                start_params.insert(
                    "task_title".to_string(),
                    Value::String("New task".to_string()),
                );

                let mut snapshot_params = serde_json::Map::new();
                if !omit_workspace {
                    snapshot_params.insert(
                        "workspace".to_string(),
                        Value::String(workspace.as_str().to_string()),
                    );
                }
                snapshot_params.insert("plan".to_string(), Value::String(target_id.clone()));

                return ai_error_with(
                    "INVALID_INPUT",
                    "macro_close_step requires a task target",
                    Some(
                        "You are targeting a plan. Start a task under the plan (portal), or set focus to a task.",
                    ),
                    vec![
                        suggest_call(
                            "tasks_macro_start",
                            "Start a task under this plan (portal).",
                            "high",
                            Value::Object(start_params),
                        ),
                        suggest_call(
                            "tasks_snapshot",
                            "Open a snapshot for this plan to confirm context (portal).",
                            "medium",
                            Value::Object(snapshot_params),
                        ),
                    ],
                );
            }
            let task_id = target_id;
            let summary = match self.store.task_steps_summary(&workspace, &task_id) {
                Ok(v) => v,
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(first_open) = summary.first_open else {
                // No open steps: treat macro_close_step as an "advance progress" macro and try to
                // finish the task deterministically (idempotent).
                close_args_obj.insert("task".to_string(), Value::String(task_id.clone()));

                // If a note is provided, attach it to the most recently completed step (if any),
                // so the note is preserved and mirrored into reasoning notes.
                if let Some(note) = note.clone() {
                    match self
                        .store
                        .task_last_completed_step_id(&workspace, &task_id)
                    {
                        Ok(Some(step_id)) => {
                            let mut note_args = close_args_obj.clone();
                            note_args.insert("step_id".to_string(), Value::String(step_id));
                            note_args.insert("note".to_string(), Value::String(note));
                            let note_resp = self.tool_tasks_note(Value::Object(note_args));
                            if !note_resp
                                .get("success")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                            {
                                return note_resp;
                            }
                            if let Some(w) = note_resp.get("warnings").and_then(|v| v.as_array()) {
                                warnings.extend(w.clone());
                            }
                            note_event = note_resp
                                .get("result")
                                .and_then(|v| v.get("event"))
                                .cloned();
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
                        }
                        Ok(None) => warnings.push(warning(
                            "NOTE_IGNORED",
                            "note was provided but the task has no steps to attach it to",
                            "Either add steps (tasks_decompose) or record the note via the reasoning tools.",
                        )),
                        Err(err) => {
                            return ai_error("STORE_ERROR", &format_store_error(err));
                        }
                    }
                }

                // If already DONE, do not emit another completion event.
                let already_done = match self.store.get_task(&workspace, &task_id) {
                    Ok(Some(t)) => t.status == "DONE",
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                if !already_done {
                    let mut complete_args = close_args_obj.clone();
                    complete_args.insert("status".to_string(), Value::String("DONE".to_string()));
                    let complete = self.tool_tasks_complete(Value::Object(complete_args));
                    if !complete
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        return complete;
                    }
                    if let Some(w) = complete.get("warnings").and_then(|v| v.as_array()) {
                        warnings.extend(w.clone());
                    }
                } else {
                    warnings.push(warning(
                        "ALREADY_DONE",
                        "task is already DONE",
                        "No action required.",
                    ));
                }

                let mut resume_args = serde_json::Map::new();
                resume_args.insert(
                    "workspace".to_string(),
                    Value::String(workspace.as_str().to_string()),
                );
                resume_args.insert("task".to_string(), Value::String(task_id.clone()));
                if let Some(agent_id) = agent_id.as_deref() {
                    resume_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
                }
                resume_args.insert(
                    "view".to_string(),
                    Value::String(view.clone().unwrap_or_else(|| "smart".to_string())),
                );
                if let Some(max_chars) = resume_max_chars {
                    resume_args.insert(
                        "max_chars".to_string(),
                        Value::Number(serde_json::Number::from(max_chars as u64)),
                    );
                }

                let resume = self.tool_tasks_resume_super(Value::Object(resume_args));
                if !resume
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    return resume;
                }
                if let Some(w) = resume.get("warnings").and_then(|v| v.as_array()) {
                    warnings.extend(w.clone());
                }

                let result = json!({
                    "task": task_id,
                    "revision": resume.get("result").and_then(|v| v.get("target")).and_then(|v| v.get("revision")).cloned().unwrap_or(Value::Null),
                    "step": Value::Null,
                    "resume": resume.get("result").cloned().unwrap_or(Value::Null),
                    "note_event": note_event.unwrap_or(Value::Null),
                    "evidence_event": Value::Null
                });

                return if warnings.is_empty() {
                    ai_ok("tasks_macro_close_step", result)
                } else {
                    ai_ok_with_warnings("tasks_macro_close_step", result, warnings, Vec::new())
                };
            };
            close_args_obj.insert("task".to_string(), Value::String(task_id));
            close_args_obj.insert("path".to_string(), Value::String(first_open.path));
        }

        let resolved_task = close_args_obj
            .get("task")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let resolved_step_id = close_args_obj
            .get("step_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let resolved_path = close_args_obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());

        if let Some(note) = note {
            close_args_obj.insert("note".to_string(), Value::String(note.clone()));
            let note_resp = self.tool_tasks_note(Value::Object(close_args_obj.clone()));
            if !note_resp
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return note_resp;
            }
            if let Some(w) = note_resp.get("warnings").and_then(|v| v.as_array()) {
                warnings.extend(w.clone());
            }
            note_event = note_resp
                .get("result")
                .and_then(|v| v.get("event"))
                .cloned();
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
        }

        if let Some(proof_value) = proof {
            let mut evidence_args = serde_json::Map::new();
            evidence_args.insert(
                "workspace".to_string(),
                Value::String(workspace.as_str().to_string()),
            );
            if let Some(task) = resolved_task.clone() {
                evidence_args.insert("task".to_string(), Value::String(task));
            }
            if let Some(step_id) = resolved_step_id.clone() {
                evidence_args.insert("step_id".to_string(), Value::String(step_id));
            }
            if let Some(path) = resolved_path.clone() {
                evidence_args.insert("path".to_string(), Value::String(path));
            }
            if let Some(expected_revision) = close_args_obj.get("expected_revision").cloned() {
                evidence_args.insert("expected_revision".to_string(), expected_revision);
            }

            // Keep proof capture copy/paste-light:
            // - If proof is a string/array of strings, treat it as checks[].
            // - If proof is an object, forward checks/items/attachments/checkpoint verbatim.
            match &proof_value {
                Value::String(s) => {
                    evidence_args.insert(
                        "checks".to_string(),
                        Value::Array(vec![Value::String(s.clone())]),
                    );
                }
                Value::Array(arr) => {
                    let mut checks = Vec::with_capacity(arr.len());
                    for item in arr {
                        let Some(s) = item.as_str() else {
                            return ai_error(
                                "INVALID_INPUT",
                                "proof array must contain only strings",
                            );
                        };
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            checks.push(Value::String(trimmed.to_string()));
                        }
                    }
                    evidence_args.insert("checks".to_string(), Value::Array(checks));
                }
                Value::Object(obj) => {
                    for key in ["items", "checks", "attachments", "checkpoint"] {
                        if let Some(v) = obj.get(key) {
                            evidence_args.insert(key.to_string(), v.clone());
                        }
                    }
                }
                _ => {
                    return ai_error(
                        "INVALID_INPUT",
                        "proof must be a string, an array of strings, or an object",
                    );
                }
            }

            // Auto-normalize proof checks to the standard receipts tags.
            // This keeps agent input syntax minimal (they can paste a command and/or a URL).
            let checks_value = evidence_args.get("checks").cloned();
            if let Some(v) = checks_value {
                let Value::Array(arr) = v else {
                    return ai_error("INVALID_INPUT", "proof.checks must be an array of strings");
                };

                let mut coerced = Vec::new();
                for item in &arr {
                    let Some(s) = item.as_str() else {
                        return ai_error(
                            "INVALID_INPUT",
                            "proof.checks array must contain only strings",
                        );
                    };
                    for line in s.lines() {
                        if let Some(c) = coerce_proof_check_line(line) {
                            coerced.push(Value::String(c));
                        }
                    }
                }
                evidence_args.insert("checks".to_string(), Value::Array(coerced));
            }

            // If checkpoint is not explicitly provided, default to attaching proof to tests.
            // This matches the most common “proof” definition (what did you run?).
            if !evidence_args.contains_key("checkpoint") {
                evidence_args.insert("checkpoint".to_string(), Value::String("tests".to_string()));
            }

            // Proof lint (soft): encourage copy/paste-ready receipts without blocking flow.
            // If the agent uses the standard tags (CMD:/LINK:), warn when one of the receipts
            // is missing or still a placeholder.
            let proof_checks = match evidence_args.get("checks") {
                None => Vec::new(),
                Some(Value::Array(arr)) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        let Some(s) = item.as_str() else {
                            return ai_error(
                                "INVALID_INPUT",
                                "proof.checks array must contain only strings",
                            );
                        };
                        out.push(s.to_string());
                    }
                    out
                }
                Some(_) => {
                    return ai_error("INVALID_INPUT", "proof.checks must be an array of strings");
                }
            };
            let lint = lint_proof_checks(&proof_checks);
            // If the agent provided a URL attachment, treat it as a LINK receipt for the soft lint.
            // This avoids false warnings when the link is provided via attachments rather than checks.
            let mut link_receipt = lint.link_receipt;
            if !link_receipt && let Some(Value::Array(arr)) = evidence_args.get("attachments") {
                for item in arr {
                    let Some(s) = item.as_str() else {
                        continue;
                    };
                    let trimmed = s.trim();
                    if trimmed.is_empty() || trimmed.contains("<fill") {
                        continue;
                    }
                    if looks_like_bare_url(trimmed) {
                        link_receipt = true;
                        break;
                    }
                }
            }

            if lint.any_tagged && (!lint.cmd_receipt || !link_receipt) {
                let mut missing = Vec::new();
                if !lint.cmd_receipt {
                    missing.push("CMD");
                }
                if !link_receipt {
                    missing.push("LINK");
                }
                warnings.push(warning(
                    "PROOF_WEAK",
                    &format!("proof incomplete: missing {}", missing.join("+")),
                    "Fill receipts as: CMD: <command you ran> and LINK: <CI run / artifact / log>.",
                ));
            }

            // Prevent “false proofs”: placeholder-only receipts must not create checkpoint evidence,
            // otherwise proof-required steps could be closed without real verification.
            let normalized_checks = normalize_proof_checks(&proof_checks);
            if normalized_checks.is_empty() {
                evidence_args.remove("checks");
            } else {
                evidence_args.insert(
                    "checks".to_string(),
                    Value::Array(
                        normalized_checks
                            .into_iter()
                            .map(Value::String)
                            .collect::<Vec<_>>(),
                    ),
                );
            }

            let attachments = match evidence_args.get("attachments") {
                None => Vec::new(),
                Some(Value::Array(arr)) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        let Some(s) = item.as_str() else {
                            return ai_error(
                                "INVALID_INPUT",
                                "proof.attachments array must contain only strings",
                            );
                        };
                        let trimmed = s.trim();
                        if !trimmed.is_empty() && !trimmed.contains("<fill") {
                            out.push(Value::String(trimmed.to_string()));
                        }
                    }
                    out
                }
                Some(_) => {
                    return ai_error(
                        "INVALID_INPUT",
                        "proof.attachments must be an array of strings",
                    );
                }
            };
            if attachments.is_empty() {
                evidence_args.remove("attachments");
            } else {
                evidence_args.insert("attachments".to_string(), Value::Array(attachments));
            }

            // Items are forwarded to tasks_evidence_capture; validate shape early so we don't
            // accidentally hide invalid inputs when filtering placeholders.
            let has_items = match evidence_args.get("items") {
                None => false,
                Some(Value::Array(arr)) => !arr.is_empty(),
                Some(_) => {
                    return ai_error("INVALID_INPUT", "proof.items must be an array of objects");
                }
            };

            let has_checks = evidence_args
                .get("checks")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| !arr.is_empty());
            let has_attachments = evidence_args
                .get("attachments")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| !arr.is_empty());
            let has_payload = has_items || has_checks || has_attachments;
            if !has_payload {
                proof_placeholder_only = true;
            } else {
                let evidence_resp = self.tool_tasks_evidence_capture(Value::Object(evidence_args));
                if !evidence_resp
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    return evidence_resp;
                }
                if let Some(w) = evidence_resp.get("warnings").and_then(|v| v.as_array()) {
                    warnings.extend(w.clone());
                }
                evidence_event = evidence_resp
                    .get("result")
                    .and_then(|v| v.get("event"))
                    .cloned();
                if let Some(revision) = evidence_resp
                    .get("result")
                    .and_then(|v| v.get("revision"))
                    .and_then(|v| v.as_i64())
                {
                    close_args_obj.insert(
                        "expected_revision".to_string(),
                        Value::Number(serde_json::Number::from(revision)),
                    );
                }
            }
        }

        // DX: default checkpoints to "gate" when closing a step (criteria+tests).
        // Contract: docs/contracts/TASKS.md specifies this default for the macro tool.
        let closing_step =
            close_args_obj.contains_key("step_id") || close_args_obj.contains_key("path");
        if closing_step && !close_args_obj.contains_key("checkpoints") {
            close_args_obj.insert("checkpoints".to_string(), Value::String("gate".to_string()));
        }

        // Strict reasoning gate (opt-in via task.reasoning_mode).
        // This is a "soft-hard" gate: it blocks closing the step unless minimum reasoning
        // discipline is present (tests + counter-position), but it provides portal-first
        // recovery suggestions so agents don't get stuck.
        if closing_step {
            let (task_id, kind, _focus) =
                match resolve_target_id(&mut self.store, &workspace, &close_args_obj) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            if kind == TaskKind::Task {
                let strict = match self.store.get_task(&workspace, &task_id) {
                    Ok(Some(task)) => task.reasoning_mode == "strict",
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };

                if strict {
                    let step_path = match resolved_path.as_deref() {
                        None => None,
                        Some(raw) => StepPath::parse(raw).ok(),
                    };
                    let step_ref = match self.store.step_resolve(
                        &workspace,
                        &task_id,
                        resolved_step_id.as_deref(),
                        step_path.as_ref(),
                    ) {
                        Ok(v) => v,
                        Err(StoreError::StepNotFound) => {
                            return ai_error("UNKNOWN_ID", "Step not found");
                        }
                        Err(StoreError::UnknownId) => {
                            return ai_error("UNKNOWN_ID", "Unknown task id");
                        }
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    let step_tag = step_tag_for(&step_ref.step_id);

                    let (reasoning_ref, _reasoning_exists) = match resolve_reasoning_ref_for_read(
                        &mut self.store,
                        &workspace,
                        &task_id,
                        TaskKind::Task,
                        false,
                    ) {
                        Ok(v) => v,
                        Err(StoreError::UnknownId) => {
                            return ai_error("UNKNOWN_ID", "Unknown task id");
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };

                    let types = bm_core::think::SUPPORTED_THINK_CARD_TYPES
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>();
                    let (cards, edges) = match self.store.graph_query(
                        &workspace,
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
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
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
                            Value::String(workspace.as_str().to_string()),
                        );
                        params.insert("target".to_string(), Value::String(task_id.clone()));
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
                                json!({ "workspace": workspace.as_str(), "name": "strict", "max_chars": 1200 }),
                            ),
                            suggest_call(
                                "think_card",
                                "Create a step-scoped hypothesis skeleton.",
                                "high",
                                Value::Object(params),
                            ),
                        ];
                        if let Some(override_input) = reasoning_override.as_ref() {
                            if let Err(resp) = apply_strict_override(
                                self,
                                &mut close_args_obj,
                                &mut warnings,
                                &mut note_event,
                                override_input,
                                vec!["STRICT_NO_HYPOTHESIS_OR_DECISION".to_string()],
                            ) {
                                return resp;
                            }
                            strict_overridden = true;
                        } else {
                            let mut override_params = args_obj.clone();
                            override_params.insert(
                                "override".to_string(),
                                json!({
                                    "reason": "Override strict gate: closing step now to unblock; will backfill hypothesis/test.",
                                    "risk": "Risk: reduced confidence; missing falsifier/counter-case could hide flaws."
                                }),
                            );
                            suggestions.push(suggest_call(
                                "tasks_macro_close_step",
                                "Override strict gate with reason+risk (escape hatch; leaves an explicit debt note).",
                                "low",
                                Value::Object(override_params),
                            ));
                            return ai_error_with(
                                "REASONING_REQUIRED",
                                "strict reasoning: step requires at least one hypothesis/decision",
                                Some(
                                    "Add a step-scoped hypothesis (and a test) before closing the step, then retry. If you must close now, retry with override={reason,risk}.",
                                ),
                                suggestions,
                            );
                        }
                    }

                    if !strict_overridden {
                        let trace_entries = match self.store.doc_show_tail(
                            &workspace,
                            &reasoning_ref.branch,
                            &reasoning_ref.trace_doc,
                            None,
                            80,
                        ) {
                            Ok(slice) => doc_entries_to_json(slice.entries),
                            Err(StoreError::InvalidInput(msg)) => {
                                return ai_error("INVALID_INPUT", msg);
                            }
                            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                        };

                        let engine = derive_reasoning_engine_step_aware(
                            EngineScope {
                                workspace: workspace.as_str(),
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
                                if let Some(override_input) = reasoning_override.as_ref() {
                                    if let Err(resp) = apply_strict_override(
                                        self,
                                        &mut close_args_obj,
                                        &mut warnings,
                                        &mut note_event,
                                        override_input,
                                        vec!["STRICT_ENGINE_NO_SIGNALS".to_string()],
                                    ) {
                                        return resp;
                                    }
                                    None
                                } else {
                                    let mut suggestions = vec![suggest_call(
                                        "think_playbook",
                                        "Load strict reasoning playbook (skepticism checklist).",
                                        "medium",
                                        json!({ "workspace": workspace.as_str(), "name": "strict", "max_chars": 1200 }),
                                    )];
                                    let mut override_params = args_obj.clone();
                                    override_params.insert(
                                        "override".to_string(),
                                        json!({
                                            "reason": "Override strict gate: closing step now to unblock; will backfill reasoning artifacts after.",
                                            "risk": "Risk: strict engine produced no signals; could mean weak evidence or missing structure."
                                        }),
                                    );
                                    suggestions.push(suggest_call(
                                        "tasks_macro_close_step",
                                        "Override strict gate with reason+risk (escape hatch; leaves an explicit debt note).",
                                        "low",
                                        Value::Object(override_params),
                                    ));
                                    return ai_error_with(
                                        "REASONING_REQUIRED",
                                        "strict reasoning: reasoning engine produced no signals/actions",
                                        Some(
                                            "Seed a minimal hypothesis+test for this step, then retry. If you must close now, retry with override={reason,risk}.",
                                        ),
                                        suggestions,
                                    );
                                }
                            }
                        };

                        if let Some(engine) = engine {
                            let shorten =
                                |s: &str, max: usize| s.chars().take(max).collect::<String>();
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
                                json!({ "workspace": workspace.as_str(), "name": "strict", "max_chars": 1200 }),
                            )];

                            let signals = engine
                                .get("signals")
                                .and_then(|v| v.as_array())
                                .cloned()
                                .unwrap_or_default();
                            for signal in signals {
                                let code =
                                    signal.get("code").and_then(|v| v.as_str()).unwrap_or("");
                                if !matches!(
                                    code,
                                    "BM4_HYPOTHESIS_NO_TEST" | "BM10_NO_COUNTER_EDGES"
                                ) {
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
                                    "workspace": workspace.as_str(),
                                    "target": task_id.clone(),
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
                                "think_card",
                                "Steelman a counter-hypothesis (step-scoped).",
                                "high",
                                json!({
                                    "workspace": workspace.as_str(),
                                    "target": task_id.clone(),
                                    "branch": reasoning_ref.branch.clone(),
                                    "trace_doc": reasoning_ref.trace_doc.clone(),
                                    "graph_doc": reasoning_ref.graph_doc.clone(),
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

                            if !missing.is_empty() {
                                let message = format!(
                                    "strict reasoning: missing discipline signals: {}",
                                    missing.join(", ")
                                );
                                let recovery = "Fix the missing reasoning artifacts (tests + counter-position) for this step, then retry closing it. If you must close now, retry with override={reason,risk}.";
                                if let Some(override_input) = reasoning_override.as_ref() {
                                    if let Err(resp) = apply_strict_override(
                                        self,
                                        &mut close_args_obj,
                                        &mut warnings,
                                        &mut note_event,
                                        override_input,
                                        missing,
                                    ) {
                                        return resp;
                                    }
                                } else {
                                    let mut override_params = args_obj.clone();
                                    override_params.insert(
                                        "override".to_string(),
                                        json!({
                                            "reason": "Override strict gate: closing step now to unblock; will backfill tests/counter-case after.",
                                            "risk": format!("Risk: missing strict discipline signals: {}", message)
                                        }),
                                    );
                                    suggestions.push(suggest_call(
                                        "tasks_macro_close_step",
                                        "Override strict gate with reason+risk (escape hatch; leaves an explicit debt note).",
                                        "low",
                                        Value::Object(override_params),
                                    ));
                                    return ai_error_with(
                                        "REASONING_REQUIRED",
                                        &message,
                                        Some(recovery),
                                        suggestions,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let close = self.tool_tasks_close_step(Value::Object(close_args_obj));
        if !close
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let error_code = close
                .get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str());
            if error_code == Some("CHECKPOINTS_NOT_CONFIRMED") {
                let checkpoint_hint = close
                    .get("suggestions")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.get("params"))
                    .and_then(|v| v.get("checkpoints"))
                    .cloned()
                    .unwrap_or(Value::String("gate".to_string()));

                let mut params = serde_json::Map::new();
                params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.as_str().to_string()),
                );
                if let Some(task) = resolved_task.clone() {
                    params.insert("task".to_string(), Value::String(task));
                }
                if let Some(step_id) = resolved_step_id.clone() {
                    params.insert("step_id".to_string(), Value::String(step_id));
                }
                if let Some(path) = resolved_path.clone() {
                    params.insert("path".to_string(), Value::String(path));
                }
                params.insert("checkpoints".to_string(), checkpoint_hint);
                if let Some(max_chars) = resume_max_chars {
                    params.insert(
                        "resume_max_chars".to_string(),
                        Value::Number(serde_json::Number::from(max_chars as u64)),
                    );
                }

                let mut patched = close.clone();
                if let Some(obj) = patched.as_object_mut() {
                    obj.insert(
                        "suggestions".to_string(),
                        Value::Array(vec![suggest_call(
                            "tasks_macro_close_step",
                            "Retry macro close with the missing checkpoints.",
                            "high",
                            Value::Object(params),
                        )]),
                    );
                }
                return patched;
            }
            if error_code == Some("PROOF_REQUIRED") {
                let omit_workspace = self.default_workspace.as_deref() == Some(workspace.as_str());
                let mut params = serde_json::Map::new();

                if !omit_workspace {
                    params.insert(
                        "workspace".to_string(),
                        Value::String(workspace.as_str().to_string()),
                    );
                }
                if let Some(task) = resolved_task.clone() {
                    params.insert("task".to_string(), Value::String(task));
                }
                if let Some(step_id) = resolved_step_id.clone() {
                    params.insert("step_id".to_string(), Value::String(step_id));
                }
                if let Some(path) = resolved_path.clone() {
                    params.insert("path".to_string(), Value::String(path));
                }
                if let Some(max_chars) = resume_max_chars {
                    params.insert(
                        "resume_max_chars".to_string(),
                        Value::Number(serde_json::Number::from(max_chars as u64)),
                    );
                }

                // Copy/paste-ready proof template: reuse the checkpoint hint from the low-level suggestion
                // so agents don't get stuck attaching proof to the wrong checkpoint family.
                let checkpoint_hint = close
                    .get("suggestions")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|s| s.get("params"))
                    .and_then(|p| p.get("checkpoint"))
                    .cloned()
                    .filter(|v| !v.is_null());
                params.insert("proof".to_string(), proof_placeholder_json(checkpoint_hint));

                let mut patched = close.clone();
                if let Some(obj) = patched.as_object_mut() {
                    if proof_placeholder_only
                        && let Some(err) = obj.get_mut("error").and_then(|v| v.as_object_mut())
                    {
                        err.insert(
                            "recovery".to_string(),
                            Value::String(
                                "Fill proof receipts (CMD + LINK) and retry; placeholder-only proof is ignored."
                                    .to_string(),
                            ),
                        );
                    }
                    obj.insert(
                        "suggestions".to_string(),
                        Value::Array(vec![suggest_call(
                            "tasks_macro_close_step",
                            "Attach proof and retry closing the step (portal).",
                            "high",
                            Value::Object(params),
                        )]),
                    );
                }
                return patched;
            }
            return close;
        }

        let task_id = match close
            .get("result")
            .and_then(|v| v.get("task"))
            .and_then(|v| v.as_str())
        {
            Some(v) => v.to_string(),
            None => return ai_error("STORE_ERROR", "close_step result missing task id"),
        };

        let mut resume_args = serde_json::Map::new();
        resume_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        resume_args.insert("task".to_string(), Value::String(task_id.clone()));
        if let Some(agent_id) = agent_id.as_deref() {
            resume_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
        }
        resume_args.insert(
            "view".to_string(),
            Value::String(view.unwrap_or_else(|| "smart".to_string())),
        );
        if let Some(max_chars) = resume_max_chars {
            resume_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        let resume = self.tool_tasks_resume_super(Value::Object(resume_args));
        if !resume
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return resume;
        }

        if let Some(w) = close.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = resume.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let result = json!({
            "task": task_id,
            "revision": close.get("result").and_then(|v| v.get("revision")).cloned().unwrap_or(Value::Null),
            "step": close.get("result").and_then(|v| v.get("step")).cloned().unwrap_or(Value::Null),
            "resume": resume.get("result").cloned().unwrap_or(Value::Null),
            "note_event": note_event.unwrap_or(Value::Null),
            "evidence_event": evidence_event.unwrap_or(Value::Null)
        });

        if warnings.is_empty() {
            ai_ok("tasks_macro_close_step", result)
        } else {
            ai_ok_with_warnings("tasks_macro_close_step", result, warnings, Vec::new())
        }
    }
}

#[derive(Clone, Debug)]
struct StrictReasoningOverride {
    reason: String,
    risk: String,
}

fn parse_strict_reasoning_override(
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
