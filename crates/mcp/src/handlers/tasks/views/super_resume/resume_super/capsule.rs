#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct HandoffCapsuleArgs<'a> {
    pub(super) toolset: Toolset,
    pub(super) workspace: &'a WorkspaceId,
    pub(super) omit_workspace: bool,
    pub(super) kind: TaskKind,
    pub(super) focus: Option<&'a str>,
    pub(super) agent_id: Option<&'a str>,
    pub(super) audit_all_lanes: bool,
    pub(super) target: &'a Value,
    pub(super) reasoning_ref: &'a Value,
    pub(super) radar: &'a Value,
    pub(super) steps_summary: Option<&'a Value>,
    pub(super) step_focus: Option<&'a Value>,
    pub(super) map_hud: Value,
    pub(super) primary_signal: Option<Value>,
    pub(super) handoff: &'a HandoffCore,
    pub(super) timeline: &'a super::timeline::TimelineEvents,
    pub(super) notes_count: usize,
    pub(super) notes_has_more: bool,
    pub(super) trace_count: usize,
    pub(super) trace_has_more: bool,
    pub(super) cards_count: usize,
    pub(super) cards_has_more: bool,
    pub(super) blockers_total: usize,
    pub(super) decisions_total: usize,
    pub(super) evidence_total: usize,
    pub(super) graph_diff_payload: Option<&'a Value>,
}

fn truncate_reason(value: &str, max_chars: usize) -> String {
    if value.len() <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn minimal_target(target: &Value) -> Value {
    let kind = target.get("kind").cloned().unwrap_or(Value::Null);
    let id = target.get("id").cloned().unwrap_or(Value::Null);
    let qualified_id = target.get("qualified_id").cloned().unwrap_or(Value::Null);
    let title = target.get("title").cloned().unwrap_or(Value::Null);
    let revision = target.get("revision").cloned().unwrap_or(Value::Null);
    let parent = target.get("parent").cloned().unwrap_or(Value::Null);

    json!({
        "kind": kind,
        "id": id,
        "qualified_id": qualified_id,
        "title": title,
        "revision": revision,
        "parent": parent
    })
}

fn minimal_step_focus(step_focus: Option<&Value>, steps_summary: Option<&Value>) -> Value {
    // Prefer the explicit step_focus payload (when available) because it can carry
    // extra room metadata (e.g. step lease).
    if let Some(step_focus) = step_focus {
        let step_id = step_focus
            .get("step")
            .and_then(|v| v.get("step_id"))
            .cloned()
            .unwrap_or(Value::Null);
        let path = step_focus
            .get("step")
            .and_then(|v| v.get("path"))
            .cloned()
            .unwrap_or(Value::Null);

        let lease = step_focus
            .get("detail")
            .and_then(|v| v.get("lease"))
            .cloned()
            .unwrap_or(Value::Null);

        let mut out = json!({ "step_id": step_id, "path": path });
        if !lease.is_null()
            && let Some(obj) = out.as_object_mut()
        {
            obj.insert("lease".to_string(), lease);
        }
        return out;
    }

    let Some(first_open) = steps_summary.and_then(|v| v.get("first_open")) else {
        return Value::Null;
    };
    let step_id = first_open.get("step_id").cloned().unwrap_or(Value::Null);
    let path = first_open.get("path").cloned().unwrap_or(Value::Null);
    json!({ "step_id": step_id, "path": path })
}

fn last_event_meta(timeline: &super::timeline::TimelineEvents) -> Value {
    let Some(last) = timeline.events.last() else {
        return Value::Null;
    };
    json!({
        "event_id": last.event_id(),
        "ts": ts_ms_to_rfc3339(last.ts_ms),
        "ts_ms": last.ts_ms,
        "type": &last.event_type,
        "path": &last.path
    })
}

fn graph_diff_meta(payload: Option<&Value>) -> Value {
    let Some(payload) = payload else {
        return Value::Null;
    };
    let Some(available) = payload.get("available").and_then(|v| v.as_bool()) else {
        return Value::Null;
    };

    let mut meta = json!({
        "available": available,
        "branch": payload.get("branch").cloned().unwrap_or(Value::Null),
        "doc": payload.get("doc").cloned().unwrap_or(Value::Null),
        "base": payload.get("base").cloned().unwrap_or(Value::Null),
        "base_source": payload.get("base_source").cloned().unwrap_or(Value::Null),
        "summary": payload.get("summary").cloned().unwrap_or(Value::Null)
    });
    if !available && let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "reason".to_string(),
            payload.get("reason").cloned().unwrap_or(Value::Null),
        );
    }
    meta
}

fn tool_available(toolset: Toolset, tool: &str) -> bool {
    match toolset {
        Toolset::Full => true,
        Toolset::Daily => matches!(
            tool,
            "status"
                | "macro_branch_note"
                | "open"
                | "tasks_macro_start"
                | "tasks_macro_close_step"
                | "tasks_snapshot"
                | "think_card"
                | "think_playbook"
        ),
        Toolset::Core => matches!(tool, "status" | "tasks_macro_start" | "tasks_snapshot"),
    }
}

fn active_step_lease_holder(step_focus: Option<&Value>) -> Option<String> {
    step_focus
        .and_then(|v| v.get("detail"))
        .and_then(|v| v.get("lease"))
        .and_then(|v| v.get("holder_agent_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn map_where_is_unknown(map_hud: &Value) -> bool {
    if map_hud
        .get("needs_anchor")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    map_hud
        .get("where")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("unknown"))
}

pub(super) fn suggested_anchor_title(task_title: Option<&str>) -> Option<String> {
    let title = task_title.unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }
    // Heuristic: prefer the "prefix" before ':' as an anchor title candidate.
    // Example: "Storage: fix migrations" -> "Storage".
    if let Some((head, _)) = title.split_once(':') {
        let head = head.trim();
        if !head.is_empty() {
            return Some(truncate_string(&redact_text(head), 80));
        }
    }
    Some(truncate_string(&redact_text(title), 80))
}

pub(super) fn derive_anchor_id_from_title(title: &str) -> String {
    // Deterministic, ascii-only slugify for `a:<slug>`:
    // - lowercased
    // - non-alnum => '-'
    // - collapse/trim '-'
    // - max 64 chars (slug)
    let raw = title.trim();
    if raw.is_empty() {
        return "a:core".to_string();
    }

    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 64 {
            break;
        }
    }

    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        return "a:core".to_string();
    }

    // Ensure the slug starts with [a-z0-9] after trimming.
    let slug = slug
        .chars()
        .skip_while(|c| !c.is_ascii_alphanumeric())
        .take(64)
        .collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return "a:core".to_string();
    }

    format!("a:{slug}")
}

fn has_open_step(steps_summary: Option<&Value>) -> bool {
    steps_summary
        .and_then(|v| v.get("first_open"))
        .is_some_and(|v| v.is_object())
}

fn recommended_map_action(args: &HandoffCapsuleArgs<'_>) -> (Value, Option<Value>) {
    if args.focus.is_none() {
        return (Value::Null, None);
    }

    let map_missing = map_where_is_unknown(&args.map_hud);

    // When the anchor is known, give a 1-command "lens" to open the anchor-scoped context.
    if !map_missing
        && let Some(where_id) = args
            .map_hud
            .get("where")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
    {
        let where_id = where_id.to_ascii_lowercase();
        if where_id.starts_with("a:") && tool_available(args.toolset, "open") {
            let tool = "open";
            let mut args_obj = serde_json::Map::new();
            if !args.omit_workspace {
                args_obj.insert(
                    "workspace".to_string(),
                    Value::String(args.workspace.as_str().to_string()),
                );
            }
            args_obj.insert("id".to_string(), Value::String(where_id));
            // Keep the anchor lens bounded for daily UX; avoid a second truncation immediately
            // after following the portal's suggestion.
            args_obj.insert("max_chars".to_string(), Value::Number(8000.into()));

            let action = json!({
                "tool": tool,
                "purpose": "open current meaning-map anchor (fast context lens)",
                "available": tool_available(args.toolset, tool),
                "args_hint": Value::Object(args_obj)
            });
            return (action, None);
        }
    }

    if !map_missing {
        return (Value::Null, None);
    }
    // Only nudge map attachment when it can immediately help navigation:
    // - for TASKs: require an open step so step-scoped attach is valid.
    // - for PLANs: do not emit a map action (plans do not have step focus).
    if args.kind == TaskKind::Task && !has_open_step(args.steps_summary) {
        return (Value::Null, None);
    }
    if args.kind == TaskKind::Plan {
        return (Value::Null, None);
    }

    let title = suggested_anchor_title(args.target.get("title").and_then(|v| v.as_str()))
        .unwrap_or_else(|| "Core".to_string());
    let anchor_id = derive_anchor_id_from_title(&title);

    // Prefer the dedicated 1-command anchor macro when available. It keeps the map stable by
    // ensuring the anchor is registered (index) and the attached note is canonical (`v:canon`).
    // Fall back to a plain think_card tag attach if the macro is not available in the current toolset.
    let prefer_macro = tool_available(args.toolset, "macro_anchor_note");
    let tool = if prefer_macro {
        "macro_anchor_note"
    } else {
        "think_card"
    };

    let mut args_obj = serde_json::Map::new();
    if !args.omit_workspace {
        args_obj.insert(
            "workspace".to_string(),
            Value::String(args.workspace.as_str().to_string()),
        );
    }

    // Bind to the focused task unless focus already matches the target.
    let target_id = args
        .target
        .get("id")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let need_target = target_id
        .as_deref()
        .zip(args.focus)
        .map(|(target, focus)| target != focus)
        .unwrap_or(true);
    if need_target && let Some(target_id) = target_id {
        args_obj.insert("target".to_string(), Value::String(target_id));
    }

    if prefer_macro {
        args_obj.insert("anchor".to_string(), Value::String(anchor_id));
        args_obj.insert("title".to_string(), Value::String(title));
        // Minimal, rent-paying default: most repos can start with "component" anchors.
        args_obj.insert("kind".to_string(), Value::String("component".to_string()));
        args_obj.insert(
            "content".to_string(),
            Value::String("Anchor attach note (invariants/risks/tests).".to_string()),
        );
        args_obj.insert("step".to_string(), Value::String("focus".to_string()));
        args_obj.insert("visibility".to_string(), Value::String("canon".to_string()));
    } else {
        args_obj.insert("step".to_string(), Value::String("focus".to_string()));
        args_obj.insert(
            "card".to_string(),
            json!({
                "text": "Anchor attach note (invariants/risks/tests).",
                "tags": [anchor_id, VIS_TAG_CANON]
            }),
        );
    }

    // If we cannot step-scope (no open step), do not emit the map action.
    if args.kind == TaskKind::Task && !has_open_step(args.steps_summary) {
        return (Value::Null, None);
    }

    let action = json!({
        "tool": tool,
        "purpose": "attach a meaning-map anchor (fix where=unknown)",
        "available": tool_available(args.toolset, tool),
        "args_hint": Value::Object(args_obj)
    });
    (action, None)
}

fn recommended_prep_action(args: &HandoffCapsuleArgs<'_>) -> (Value, Option<Value>) {
    if args.focus.is_none() {
        return (Value::Null, None);
    }

    match args.kind {
        TaskKind::Task => {
            let status = args
                .target
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status == "DONE" {
                return (Value::Null, None);
            }

            let total_steps = args
                .steps_summary
                .and_then(|v| v.get("total"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if total_steps <= 0 {
                return (Value::Null, None);
            }

            let first_open = args.steps_summary.and_then(|v| v.get("first_open"));
            if first_open.is_none() {
                return (Value::Null, None);
            }

            let target_id = args
                .target
                .get("id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let need_target = target_id
                .as_deref()
                .zip(args.focus)
                .map(|(target, focus)| target != focus)
                .unwrap_or(true);

            let mut args_obj = serde_json::Map::new();
            match args.toolset {
                Toolset::Daily => {
                    // Daily portal UX: keep the prep loop inside the daily toolset (no disclosure).
                    //
                    // Flagship DX: when we already know `where=a:*`, prefer a 1-command preflight that
                    // *writes* a step-scoped, anchor-tagged artifact (draft) so nothing is lost across
                    // /compact or restarts. This stays low-noise because step-scoped drafts are shown
                    // only while that step is focused.
                    //
                    // Additionally: when a strict/deep reasoning gate is likely to block closure,
                    // prioritize a 1-command fix-up action over the generic skeptic preflight.

                    let reasoning_mode = args
                        .target
                        .get("reasoning_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("normal")
                        .trim()
                        .to_ascii_lowercase();
                    let signal_code = args
                        .primary_signal
                        .as_ref()
                        .and_then(|v| v.get("code"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let signal_ref_id = args
                        .primary_signal
                        .as_ref()
                        .and_then(|v| v.get("refs"))
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|r| r.get("id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if matches!(reasoning_mode.as_str(), "deep" | "strict")
                        && matches!(
                            signal_code.as_str(),
                            "DEEP_NEEDS_RESOLVED_DECISION"
                                | "DEEP_MIN_2_HYPOTHESES"
                                | "BM4_HYPOTHESIS_NO_TEST"
                                | "BM10_NO_COUNTER_EDGES"
                        )
                    {
                        let tool = "think_card";
                        if !args.omit_workspace {
                            args_obj.insert(
                                "workspace".to_string(),
                                Value::String(args.workspace.as_str().to_string()),
                            );
                        }
                        if need_target && let Some(task) = target_id.clone() {
                            args_obj.insert("target".to_string(), Value::String(task));
                        }
                        args_obj.insert("step".to_string(), Value::String("focus".to_string()));

                        let (purpose, card, supports, blocks) = match signal_code.as_str() {
                            "DEEP_NEEDS_RESOLVED_DECISION" => (
                                "record a resolved synthesis decision (deep gate)",
                                json!({
                                    "type": "decision",
                                    "title": "Decision:TBD",
                                    "text": "Synthesis: winner + tradeoffs + rollback/stop rule + what would change your mind.",
                                    "status": "resolved",
                                    "tags": ["bm-deep"]
                                }),
                                None,
                                None,
                            ),
                            "DEEP_MIN_2_HYPOTHESES" => (
                                "add a second hypothesis branch (deep gate)",
                                json!({
                                    "type": "hypothesis",
                                    "title": "Hypothesis(alt):TBD",
                                    "text": "Write the best alternative hypothesis; add one disconfirming test idea.",
                                    "status": "open",
                                    "tags": ["bm-deep", "branch"]
                                }),
                                None,
                                None,
                            ),
                            "BM4_HYPOTHESIS_NO_TEST" => (
                                "add a falsifier test stub for the current hypothesis (strict gate)",
                                json!({
                                    "type": "test",
                                    "title": "Test:TBD",
                                    "text": "Define the smallest runnable check for this hypothesis.",
                                    "status": "open",
                                    "tags": ["bm4"]
                                }),
                                signal_ref_id.clone(),
                                None,
                            ),
                            "BM10_NO_COUNTER_EDGES" => (
                                "steelman a counter-hypothesis (strict gate)",
                                json!({
                                    "type": "hypothesis",
                                    "title": "Counter-hypothesis:TBD",
                                    "text": "Steelman the opposite case; include 1 disconfirming test idea.",
                                    "status": "open",
                                    "tags": ["bm7", "counter"]
                                }),
                                None,
                                signal_ref_id.clone(),
                            ),
                            _ => ("", Value::Null, None, None),
                        };

                        if purpose.is_empty() || card.is_null() {
                            return (Value::Null, None);
                        }
                        args_obj.insert("card".to_string(), card);
                        if let Some(id) = supports {
                            args_obj.insert(
                                "supports".to_string(),
                                Value::Array(vec![Value::String(id)]),
                            );
                        }
                        if let Some(id) = blocks {
                            args_obj.insert(
                                "blocks".to_string(),
                                Value::Array(vec![Value::String(id)]),
                            );
                        }

                        let action = json!({
                            "tool": tool,
                            "purpose": purpose,
                            "available": tool_available(args.toolset, tool),
                            "args_hint": Value::Object(args_obj)
                        });
                        return (action, None);
                    }

                    let where_id = args
                        .map_hud
                        .get("where")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if !where_id.is_empty()
                        && !where_id.eq_ignore_ascii_case("unknown")
                        && where_id.starts_with(ANCHOR_TAG_PREFIX)
                    {
                        let tool = "think_card";
                        if !args.omit_workspace {
                            args_obj.insert(
                                "workspace".to_string(),
                                Value::String(args.workspace.as_str().to_string()),
                            );
                        }
                        if need_target && let Some(task) = target_id {
                            args_obj.insert("target".to_string(), Value::String(task));
                        }
                        args_obj.insert("step".to_string(), Value::String("focus".to_string()));
                        args_obj.insert(
                            "card".to_string(),
                            json!({
                                "type": "test",
                                "title": "Skeptic preflight (counter → falsifier → stop)",
                                "text": "counter-hypothesis: TBD\nfalsifier test: TBD\nstop criteria: TBD",
                                "tags": [where_id, VIS_TAG_DRAFT, "skeptic:preflight"]
                            }),
                        );

                        let action = json!({
                            "tool": tool,
                            "purpose": "record a step-scoped skeptic preflight (draft; anchor-tagged)",
                            "available": tool_available(args.toolset, tool),
                            "args_hint": Value::Object(args_obj)
                        });
                        (action, None)
                    } else {
                        // Fallback: when `where` is unknown, suggest the deterministic skeptic playbook.
                        let tool = "think_playbook";
                        let reasoning_mode = args
                            .target
                            .get("reasoning_mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("normal");
                        let playbook = match reasoning_mode {
                            "deep" => "deep",
                            "strict" => "strict",
                            _ => "skeptic",
                        };
                        if !args.omit_workspace {
                            args_obj.insert(
                                "workspace".to_string(),
                                Value::String(args.workspace.as_str().to_string()),
                            );
                        }
                        args_obj.insert("name".to_string(), Value::String(playbook.to_string()));
                        let action = json!({
                            "tool": tool,
                            "purpose": "skeptic loop before closing (counter-hypothesis → falsifier → stop criteria)",
                            "available": tool_available(args.toolset, tool),
                            "args_hint": Value::Object(args_obj)
                        });
                        (action, None)
                    }
                }
                Toolset::Full => {
                    // Full surface: suggest the structured thinking pipeline (step-scoped).
                    let tool = "think_pipeline";
                    if !args.omit_workspace {
                        args_obj.insert(
                            "workspace".to_string(),
                            Value::String(args.workspace.as_str().to_string()),
                        );
                    }
                    if need_target && let Some(task) = target_id {
                        args_obj.insert("target".to_string(), Value::String(task));
                    }

                    // Deterministic DX: step="focus" makes this a copy/paste-safe two-step flow.
                    args_obj.insert("step".to_string(), Value::String("focus".to_string()));
                    if let Some(agent_id) = args.agent_id {
                        args_obj
                            .insert("agent_id".to_string(), Value::String(agent_id.to_string()));
                    }

                    // Keep capsule small: provide a single stage placeholder; other stages are optional.
                    args_obj.insert("frame".to_string(), Value::String("TBD".to_string()));
                    args_obj.insert("note_decision".to_string(), Value::Bool(true));
                    args_obj.insert(
                        "note_title".to_string(),
                        Value::String("Decision".to_string()),
                    );
                    args_obj.insert("note_format".to_string(), Value::String("text".to_string()));

                    let action = json!({
                        "tool": tool,
                        "purpose": "prepare step-scoped thinking (frame→hypothesis→test→evidence→decision)",
                        "available": tool_available(args.toolset, tool),
                        "args_hint": Value::Object(args_obj)
                    });
                    (action, None)
                }
                Toolset::Core => (Value::Null, None),
            }
        }
        TaskKind::Plan => (Value::Null, None),
    }
}

fn recommended_action(args: &HandoffCapsuleArgs<'_>) -> (Value, Option<Value>) {
    if args.focus.is_none() {
        let tool = "tasks_macro_start";
        let mut action = json!({
            "tool": tool,
            "purpose": "establish focus (start a new task)",
            "available": tool_available(args.toolset, tool),
            "args_hint": {
                "workspace": args.workspace.as_str(),
                "plan_template": "principal-plan",
                "template": "principal-task",
                "task_title": "..."
            }
        });
        if args.omit_workspace
            && let Some(obj) = action.get_mut("args_hint").and_then(|v| v.as_object_mut())
        {
            obj.remove("workspace");
        }
        return (action, None);
    }

    match args.kind {
        TaskKind::Task => {
            let status = args
                .target
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status == "DONE" {
                return (Value::Null, None);
            }

            let total_steps = args
                .steps_summary
                .and_then(|v| v.get("total"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if total_steps <= 0 {
                let tool = "tasks_decompose";
                let needs_toolset = if tool_available(args.toolset, tool) {
                    None
                } else {
                    Some(Value::String("full".to_string()))
                };
                let mut action = json!({
                    "tool": tool,
                    "purpose": "add steps to this task",
                    "available": tool_available(args.toolset, tool),
                    "args_hint": {
                        "workspace": args.workspace.as_str(),
                        "task": args.target.get("id").cloned().unwrap_or(Value::Null),
                        "steps": [
                            { "title": "...", "success_criteria": ["..."], "tests": ["..."] }
                        ]
                    }
                });
                if args.omit_workspace
                    && let Some(obj) = action.get_mut("args_hint").and_then(|v| v.as_object_mut())
                {
                    obj.remove("workspace");
                }
                let escalation = needs_toolset.map(|ts| {
                    json!({
                        "required": true,
                        "toolset": ts,
                        "reason": "task has no steps; need step-level tools"
                    })
                });
                return (action, escalation);
            }

            let first_open = args.steps_summary.and_then(|v| v.get("first_open"));
            let first_path = first_open
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());

            if let Some(_path) = first_path {
                // Multi-agent safety: when the current step is leased by another agent,
                // avoid wasting a RTT on a guaranteed step mutation failure.
                if let Some(holder) = active_step_lease_holder(args.step_focus) {
                    let owned_by_me = args
                        .agent_id
                        .map(|me| me == holder.as_str())
                        .unwrap_or(false);
                    if !owned_by_me {
                        let tool = "tasks_step_lease_get";
                        let required_toolset = if tool_available(args.toolset, tool) {
                            None
                        } else {
                            Some(Value::String("full".to_string()))
                        };

                        let target_id = args
                            .target
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(|v| v.to_string());
                        let need_task = target_id
                            .as_deref()
                            .zip(args.focus)
                            .map(|(target, focus)| target != focus)
                            .unwrap_or(true);

                        let mut args_obj = serde_json::Map::new();
                        if !args.omit_workspace {
                            args_obj.insert(
                                "workspace".to_string(),
                                Value::String(args.workspace.as_str().to_string()),
                            );
                        }
                        if need_task && let Some(task) = target_id.clone() {
                            args_obj.insert("task".to_string(), Value::String(task));
                        }
                        if let Some(step_id) = first_open
                            .and_then(|v| v.get("step_id"))
                            .and_then(|v| v.as_str())
                        {
                            args_obj
                                .insert("step_id".to_string(), Value::String(step_id.to_string()));
                        }
                        if let Some(path) = first_open
                            .and_then(|v| v.get("path"))
                            .and_then(|v| v.as_str())
                        {
                            args_obj.insert("path".to_string(), Value::String(path.to_string()));
                        }
                        if let Some(agent_id) = args.agent_id {
                            args_obj.insert(
                                "agent_id".to_string(),
                                Value::String(agent_id.to_string()),
                            );
                        }

                        let action = json!({
                            "tool": tool,
                            "purpose": format!("inspect step lease (held by {holder})"),
                            "available": tool_available(args.toolset, tool),
                            "args": Value::Object(args_obj)
                        });
                        let escalation = required_toolset.map(|ts| {
                            json!({
                                "required": true,
                                "toolset": ts,
                                "reason": "lease ops are hidden in the current toolset"
                            })
                        });
                        return (action, escalation);
                    }
                }

                let tool = "tasks_macro_close_step";
                let required_toolset = if tool_available(args.toolset, tool) {
                    None
                } else {
                    Some(Value::String("daily".to_string()))
                };

                let target_id = args
                    .target
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let need_task = target_id
                    .as_deref()
                    .zip(args.focus)
                    .map(|(target, focus)| target != focus)
                    .unwrap_or(true);

                let mut args_obj = serde_json::Map::new();
                if !args.omit_workspace {
                    args_obj.insert(
                        "workspace".to_string(),
                        Value::String(args.workspace.as_str().to_string()),
                    );
                }
                if need_task && let Some(task) = target_id.clone() {
                    args_obj.insert("task".to_string(), Value::String(task));
                }

                let require_security = first_open
                    .and_then(|v| v.get("require_security"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let require_perf = first_open
                    .and_then(|v| v.get("require_perf"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let require_docs = first_open
                    .and_then(|v| v.get("require_docs"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let security_confirmed = first_open
                    .and_then(|v| v.get("security_confirmed"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let perf_confirmed = first_open
                    .and_then(|v| v.get("perf_confirmed"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let docs_confirmed = first_open
                    .and_then(|v| v.get("docs_confirmed"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let proof_tests_mode = first_open
                    .and_then(|v| v.get("proof_tests_mode"))
                    .and_then(|v| v.as_str());
                let proof_security_mode = first_open
                    .and_then(|v| v.get("proof_security_mode"))
                    .and_then(|v| v.as_str());
                let proof_perf_mode = first_open
                    .and_then(|v| v.get("proof_perf_mode"))
                    .and_then(|v| v.as_str());
                let proof_docs_mode = first_open
                    .and_then(|v| v.get("proof_docs_mode"))
                    .and_then(|v| v.as_str());

                // Portal DX: always include a copy/paste-safe default for `tasks_macro_close_step`.
                // If extra confirmations become required (e.g., checkpoint evidence exists), include
                // them explicitly so the macro can succeed in one call.
                let missing_security = (require_security || proof_security_mode == Some("require"))
                    && !security_confirmed;
                let missing_perf =
                    (require_perf || proof_perf_mode == Some("require")) && !perf_confirmed;
                let missing_docs =
                    (require_docs || proof_docs_mode == Some("require")) && !docs_confirmed;
                if missing_security || missing_perf || missing_docs {
                    let mut cp = serde_json::Map::new();
                    cp.insert("criteria".to_string(), Value::Bool(true));
                    cp.insert("tests".to_string(), Value::Bool(true));
                    if missing_security {
                        cp.insert("security".to_string(), Value::Bool(true));
                    }
                    if missing_perf {
                        cp.insert("perf".to_string(), Value::Bool(true));
                    }
                    if missing_docs {
                        cp.insert("docs".to_string(), Value::Bool(true));
                    }
                    args_obj.insert("checkpoints".to_string(), Value::Object(cp));
                } else {
                    args_obj.insert("checkpoints".to_string(), Value::String("gate".to_string()));
                }

                // Proof-first (hybrid): inject proof placeholders only when the step explicitly requires proofs.
                // Keep it copy/paste-ready and standardized so agents can attach real receipts quickly.
                let proof_tests_present = first_open
                    .and_then(|v| v.get("proof_tests_present"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let proof_security_present = first_open
                    .and_then(|v| v.get("proof_security_present"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let proof_perf_present = first_open
                    .and_then(|v| v.get("proof_perf_present"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let proof_docs_present = first_open
                    .and_then(|v| v.get("proof_docs_present"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let missing_tests = proof_tests_mode == Some("require") && !proof_tests_present;
                let missing_security =
                    proof_security_mode == Some("require") && !proof_security_present;
                let missing_perf = proof_perf_mode == Some("require") && !proof_perf_present;
                let missing_docs = proof_docs_mode == Some("require") && !proof_docs_present;

                if missing_tests || missing_security || missing_perf || missing_docs {
                    let checkpoint = proof_checkpoint_value_for_missing(
                        missing_tests,
                        missing_security,
                        missing_perf,
                        missing_docs,
                    );
                    args_obj.insert("proof".to_string(), proof_placeholder_json(checkpoint));
                }

                let action = json!({
                    "tool": tool,
                    "purpose": "close next step (confirm checkpoints + done)",
                    "available": tool_available(args.toolset, tool),
                    "args": Value::Object(args_obj)
                });
                let escalation = required_toolset.map(|ts| {
                    json!({
                        "required": true,
                        "toolset": ts,
                        "reason": "progress ops are hidden in the current toolset"
                    })
                });
                return (action, escalation);
            }

            let open_steps = args
                .steps_summary
                .and_then(|v| v.get("open"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if open_steps <= 0 {
                // No open steps: the next right action is to finish the task (status DONE).
                // In full toolsets, prefer the explicit finish macro (includes handoff).
                // In portal toolsets, keep this inside the progress macro to avoid expanding the surface.
                let tool = match args.toolset {
                    Toolset::Full => "tasks_macro_finish",
                    Toolset::Daily | Toolset::Core => "tasks_macro_close_step",
                };

                let required_toolset = if tool_available(args.toolset, tool) {
                    None
                } else {
                    Some(Value::String("daily".to_string()))
                };

                let target_id = args
                    .target
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let need_task = target_id
                    .as_deref()
                    .zip(args.focus)
                    .map(|(target, focus)| target != focus)
                    .unwrap_or(true);

                let mut args_obj = serde_json::Map::new();
                if !args.omit_workspace {
                    args_obj.insert(
                        "workspace".to_string(),
                        Value::String(args.workspace.as_str().to_string()),
                    );
                }
                if need_task && let Some(task) = target_id {
                    args_obj.insert("task".to_string(), Value::String(task));
                }

                let action = json!({
                    "tool": tool,
                    "purpose": if tool == "tasks_macro_finish" { "finish task + handoff" } else { "finish task (set status DONE)" },
                    "available": tool_available(args.toolset, tool),
                    "args": Value::Object(args_obj)
                });
                let escalation = required_toolset.map(|ts| {
                    json!({
                        "required": true,
                        "toolset": ts,
                        "reason": "progress ops are hidden in the current toolset"
                    })
                });
                return (action, escalation);
            }

            (Value::Null, None)
        }
        TaskKind::Plan => {
            let tool = "tasks_plan";
            let escalation = if tool_available(args.toolset, tool) {
                None
            } else {
                Some(json!({
                    "required": true,
                    "toolset": "full",
                    "reason": "plan checklist ops are not in the current toolset"
                }))
            };
            let mut action = json!({
                "tool": tool,
                "purpose": "advance plan checklist",
                "available": tool_available(args.toolset, tool),
                "args_hint": {
                    "workspace": args.workspace.as_str(),
                    "plan": args.target.get("id").cloned().unwrap_or(Value::Null),
                    "advance": true
                }
            });
            if args.omit_workspace
                && let Some(obj) = action.get_mut("args_hint").and_then(|v| v.as_object_mut())
            {
                obj.remove("workspace");
            }
            (action, escalation)
        }
    }
}

pub(super) fn build_handoff_capsule(args: HandoffCapsuleArgs<'_>) -> Value {
    let why = args
        .radar
        .get("why")
        .and_then(|v| v.as_str())
        .map(|v| truncate_reason(v, 256))
        .map(Value::String)
        .unwrap_or(Value::Null);

    let mut radar = args.radar.clone();
    if let Some(obj) = radar.as_object_mut() {
        obj.insert("why".to_string(), why);
    }

    let (map_action, map_escalation) = recommended_map_action(&args);
    let (prep_action, prep_escalation) = recommended_prep_action(&args);
    let (action, escalation) = recommended_action(&args);
    let lane = if args.audit_all_lanes {
        json!({ "kind": "all" })
    } else {
        lane_meta_value(None)
    };
    json!({
        "type": "handoff_capsule",
        "version": 1,
        "toolset": args.toolset.as_str(),
        "workspace": args.workspace.as_str(),
        "focus": args.focus.map(|v| Value::String(v.to_string())).unwrap_or(Value::Null),
        "where": {
            "lane": lane,
            "step_focus": minimal_step_focus(args.step_focus, args.steps_summary),
            "map": args.map_hud
        },
        "target": minimal_target(args.target),
        "reasoning_ref": args.reasoning_ref,
        "radar": radar,
        "reasoning_signal": args.primary_signal.clone().unwrap_or(Value::Null),
        "handoff": {
            "done": &args.handoff.done,
            "remaining": &args.handoff.remaining,
            "risks": &args.handoff.risks
        },
        "counts": {
            "timeline_events": args.timeline.events.len(),
            "notes": { "count": args.notes_count, "has_more": args.notes_has_more },
            "trace": { "count": args.trace_count, "has_more": args.trace_has_more },
            "cards": { "count": args.cards_count, "has_more": args.cards_has_more },
            "signals": {
                "blockers": args.blockers_total,
                "decisions": args.decisions_total,
                "evidence": args.evidence_total
            }
        },
        "last_event": last_event_meta(args.timeline),
        "graph_diff": graph_diff_meta(args.graph_diff_payload),
        "map_action": map_action,
        "map_escalation": map_escalation.unwrap_or(Value::Null),
        "prep_action": prep_action,
        "prep_escalation": prep_escalation.unwrap_or(Value::Null),
        "action": action,
        "escalation": escalation.unwrap_or(Value::Null)
    })
}
