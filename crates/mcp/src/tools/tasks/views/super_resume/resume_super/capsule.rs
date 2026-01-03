#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct HandoffCapsuleArgs<'a> {
    pub(super) toolset: Toolset,
    pub(super) workspace: &'a WorkspaceId,
    pub(super) omit_workspace: bool,
    pub(super) kind: TaskKind,
    pub(super) focus: Option<&'a str>,
    pub(super) target: &'a Value,
    pub(super) reasoning_ref: &'a Value,
    pub(super) radar: &'a Value,
    pub(super) steps_summary: Option<&'a Value>,
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
                | "tasks_macro_start"
                | "tasks_macro_close_step"
                | "tasks_snapshot"
        ),
        Toolset::Core => matches!(tool, "status" | "tasks_macro_start" | "tasks_snapshot"),
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

                let missing_security = require_security
                    && !first_open
                        .and_then(|v| v.get("security_confirmed"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                let missing_perf = require_perf
                    && !first_open
                        .and_then(|v| v.get("perf_confirmed"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                let missing_docs = require_docs
                    && !first_open
                        .and_then(|v| v.get("docs_confirmed"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

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
                }

                // Proof-first (hybrid): inject proof placeholders only when the step explicitly requires proofs.
                // Keep it copy/paste-ready and standardized so agents can attach real receipts quickly.
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
                // Daily driver keeps this inside the progress macro to avoid expanding the surface.
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
                if need_task && let Some(task) = target_id {
                    args_obj.insert("task".to_string(), Value::String(task));
                }

                let action = json!({
                    "tool": tool,
                    "purpose": "finish task (set status DONE)",
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

    let (action, escalation) = recommended_action(&args);
    json!({
        "type": "handoff_capsule",
        "version": 1,
        "toolset": args.toolset.as_str(),
        "workspace": args.workspace.as_str(),
        "focus": args.focus.map(|v| Value::String(v.to_string())).unwrap_or(Value::Null),
        "target": minimal_target(args.target),
        "reasoning_ref": args.reasoning_ref,
        "radar": radar,
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
        "action": action,
        "escalation": escalation.unwrap_or(Value::Null)
    })
}
