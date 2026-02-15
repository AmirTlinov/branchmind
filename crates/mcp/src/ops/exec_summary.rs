#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, OpError, OpResponse, handler_to_op_response};
use serde_json::{Value, json};

fn issue_is_critical_regression(issue: &Value) -> bool {
    let severity = issue
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(severity.as_str(), "critical" | "error") {
        return true;
    }
    let code = issue
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    code.contains("REGRESSION")
}

pub(crate) fn extract_critical_regressions(issues: &[Value]) -> Vec<Value> {
    issues
        .iter()
        .filter(|issue| issue_is_critical_regression(issue))
        .cloned()
        .collect::<Vec<_>>()
}

pub(crate) fn build_tasks_exec_summary(
    server: &mut crate::McpServer,
    intent: String,
    workspace: Option<&str>,
    args: Value,
) -> OpResponse {
    let handoff_raw = server.tool_tasks_handoff(args.clone());
    let handoff = handler_to_op_response(&intent, workspace, handoff_raw);
    if handoff.error.is_some() {
        return handoff;
    }

    let lint_raw = server.tool_tasks_lint(args);
    let lint = handler_to_op_response(&intent, workspace, lint_raw);
    if lint.error.is_some() {
        return lint;
    }

    let lint_issues = lint
        .result
        .get("issues")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let critical_regressions = extract_critical_regressions(&lint_issues);
    let critical_regressions_count = critical_regressions.len();
    let workspace = workspace.unwrap_or_default();

    let result = json!({
        "workspace": workspace,
        "target": handoff.result.get("target").cloned().unwrap_or(serde_json::Value::Null),
        "exec_summary": {
            "radar": handoff.result.get("radar").cloned().unwrap_or(serde_json::Value::Null),
            "handoff": handoff.result.get("handoff").cloned().unwrap_or(serde_json::Value::Null),
            "steps": handoff.result.get("steps").cloned().unwrap_or(serde_json::Value::Null),
        },
        "critical_regressions": critical_regressions,
        "critical_regressions_count": critical_regressions_count,
        "lint_summary": lint.result.get("summary").cloned().unwrap_or(serde_json::Value::Null),
        "lint_status": lint.result.get("context_health").and_then(|v| v.get("status")).cloned().unwrap_or(serde_json::Value::Null),
        "source": {
            "exec_summary": "tasks.handoff",
            "regressions": "tasks.lint"
        }
    });

    let mut resp = OpResponse::success(intent, result);
    resp.warnings.extend(handoff.warnings);
    resp.warnings.extend(lint.warnings);
    resp.actions.extend(handoff.actions);
    resp.actions.extend(lint.actions);
    resp
}

fn append_actions_dedupe(dst: &mut Vec<Action>, src: Vec<Action>) {
    let mut seen = dst
        .iter()
        .map(|a| a.action_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for action in src {
        if seen.insert(action.action_id.clone()) {
            dst.push(action);
        }
    }
}

fn parse_optional_u64(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<u64>, OpError> {
    let Some(v) = args_obj.get(key) else {
        return Ok(None);
    };
    match v {
        Value::Null => Ok(None),
        Value::Number(n) => n.as_u64().map(Some).ok_or_else(|| OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected non-negative integer"),
            recovery: Some(format!("Provide args.{key} as an integer >= 0")),
        }),
        _ => Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("{key}: expected non-negative integer"),
            recovery: Some(format!("Provide args.{key} as an integer >= 0")),
        }),
    }
}

fn compact_action(action: &Action) -> Value {
    json!({
        "action_id": action.action_id,
        "tool": action.tool,
        "priority": action.priority.as_str(),
        "why": action.why
    })
}

fn is_critical_inbox_item(item: &Value) -> bool {
    let severity = item
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    matches!(severity.as_str(), "P0" | "P1")
}

fn parse_action_priority(raw: Option<&str>) -> ActionPriority {
    let raw = raw.unwrap_or("low").trim();
    if raw.eq_ignore_ascii_case("high") {
        ActionPriority::High
    } else if raw.eq_ignore_ascii_case("medium") {
        ActionPriority::Medium
    } else {
        ActionPriority::Low
    }
}

fn collect_center_actions(center_obj: &serde_json::Map<String, Value>) -> Vec<Action> {
    let workspace = center_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let Some(items) = center_obj.get("actions").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::<Action>::new();
    for (idx, item) in items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let cmd = obj.get("cmd").and_then(|v| v.as_str()).unwrap_or_default();
        let op = obj
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or(if cmd.is_empty() { "status" } else { "call" });
        let args_payload = obj.get("args").cloned().unwrap_or_else(|| json!({}));
        let budget_profile = obj
            .get("budget_profile")
            .and_then(|v| v.as_str())
            .unwrap_or("portal");
        let portal_view = obj
            .get("portal_view")
            .and_then(|v| v.as_str())
            .unwrap_or("compact");
        let priority = parse_action_priority(obj.get("priority").and_then(|v| v.as_str()));
        let why = obj
            .get("reason")
            .or_else(|| obj.get("why"))
            .and_then(|v| v.as_str())
            .unwrap_or("Recommended follow-up action from jobs.control.center")
            .to_string();

        let args = if op == "call" {
            json!({
                "workspace": workspace,
                "op": "call",
                "cmd": cmd,
                "args": args_payload,
                "budget_profile": budget_profile,
                "portal_view": portal_view
            })
        } else {
            json!({
                "workspace": workspace,
                "op": op,
                "args": args_payload,
                "budget_profile": budget_profile,
                "portal_view": portal_view
            })
        };
        let action_id = if cmd.is_empty() {
            format!("jobs.exec.summary.center::{idx}::{op}")
        } else {
            format!("jobs.exec.summary.center::{idx}::{cmd}")
        };
        out.push(Action {
            action_id,
            priority,
            tool: "jobs".to_string(),
            args,
            why,
            risk: "Низкий".to_string(),
        });
    }
    out
}

pub(crate) fn build_jobs_exec_summary(
    server: &mut crate::McpServer,
    intent: String,
    workspace: Option<&str>,
    args: Value,
) -> OpResponse {
    let args_obj = args.as_object().cloned().unwrap_or_default();
    let include_details = args_obj
        .get("include_details")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_regressions = match parse_optional_u64(&args_obj, "max_regressions") {
        Ok(v) => v.unwrap_or(3).clamp(1, 20) as usize,
        Err(err) => return OpResponse::error(intent, err),
    };

    let mut center_args = serde_json::Map::new();
    if let Some(ws) = workspace {
        center_args.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    if let Some(task) = args_obj.get("task") {
        center_args.insert("task".to_string(), task.clone());
    }
    if let Some(anchor) = args_obj.get("anchor") {
        center_args.insert("anchor".to_string(), anchor.clone());
    }
    if let Some(view) = args_obj.get("view") {
        center_args.insert("view".to_string(), view.clone());
    } else {
        center_args.insert("view".to_string(), Value::String("smart".to_string()));
    }
    if let Some(limit) = args_obj.get("limit") {
        center_args.insert("limit".to_string(), limit.clone());
    } else {
        center_args.insert("limit".to_string(), Value::Number(20.into()));
    }
    if let Some(stall_after_s) = args_obj.get("stall_after_s") {
        center_args.insert("stall_after_s".to_string(), stall_after_s.clone());
    }

    let center_raw = server.tool_tasks_jobs_control_center(Value::Object(center_args));
    let center = handler_to_op_response(&intent, workspace, center_raw);
    if center.error.is_some() {
        return center;
    }

    let center_obj = center.result.as_object().cloned().unwrap_or_default();
    let inbox_items = center_obj
        .get("inbox")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let critical_all = inbox_items
        .iter()
        .filter(|item| is_critical_inbox_item(item))
        .cloned()
        .collect::<Vec<_>>();
    let critical_regressions_count = critical_all.len();
    let critical_regressions = critical_all
        .into_iter()
        .take(max_regressions)
        .collect::<Vec<_>>();

    let jobs = center_obj
        .get("jobs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let running = jobs
        .iter()
        .filter(|job| {
            job.get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("RUNNING"))
        })
        .count();
    let queued = jobs
        .iter()
        .filter(|job| {
            job.get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("QUEUED"))
        })
        .count();

    let execution_health = center_obj
        .get("execution_health")
        .cloned()
        .unwrap_or(Value::Null);
    let pipeline_health = center_obj
        .get("pipeline_health")
        .cloned()
        .unwrap_or(Value::Null);
    let runner_status = execution_health
        .get("runner_status")
        .cloned()
        .unwrap_or(Value::Null);
    let runner_status_name = runner_status
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let live_count = runner_status
        .get("live_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let needs_manager = execution_health
        .get("needs_manager")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let needs_proof = execution_health
        .get("needs_proof")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let stalled_jobs = execution_health
        .get("stalled_jobs")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let inbox_count = center_obj
        .get("inbox")
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let headline = if critical_regressions_count > 0 {
        format!(
            "Есть {} критических attention-сигналов (P0/P1) — разберите их первыми.",
            critical_regressions_count
        )
    } else if needs_manager > 0 || needs_proof > 0 {
        "Есть manager/proof attention, но без P0/P1.".to_string()
    } else if running > 0 {
        format!("Исполнение идёт ровно: RUNNING={running}, QUEUED={queued}.")
    } else if queued > 0 && live_count == 0 {
        format!("Очередь есть (QUEUED={queued}), но runner сейчас не live.")
    } else if queued > 0 {
        format!("Очередь готова к запуску: QUEUED={queued}.")
    } else {
        "Очередь пустая, критических сигналов нет.".to_string()
    };

    let mut actions = Vec::<Action>::new();
    append_actions_dedupe(&mut actions, center.actions.clone());
    append_actions_dedupe(&mut actions, collect_center_actions(&center_obj));
    if critical_regressions_count > 0
        && let Some(job_id) = critical_regressions
            .first()
            .and_then(|item| item.get("job_id"))
            .and_then(|v| v.as_str())
    {
        append_actions_dedupe(
            &mut actions,
            vec![Action {
                action_id: format!("jobs.exec.summary.open::{job_id}"),
                priority: ActionPriority::High,
                tool: "jobs".to_string(),
                args: json!({
                    "workspace": workspace,
                    "op": "open",
                    "args": { "job": job_id },
                    "budget_profile": "portal",
                    "portal_view": "compact"
                }),
                why: "Откройте критический job и снимите blocker/риск первым действием."
                    .to_string(),
                risk: "Низкий".to_string(),
            }],
        );
    }
    if queued > 0 && live_count == 0 {
        append_actions_dedupe(
            &mut actions,
            vec![Action {
                action_id: "jobs.exec.summary.runner.start".to_string(),
                priority: ActionPriority::High,
                tool: "jobs".to_string(),
                args: json!({
                    "workspace": workspace,
                    "op": "runner.start",
                    "args": {},
                    "budget_profile": "portal",
                    "portal_view": "compact"
                }),
                why: "Запустите runner: есть очередь без live-исполнителя.".to_string(),
                risk: "Низкий".to_string(),
            }],
        );
    }

    let mut result_obj = serde_json::Map::new();
    result_obj.insert(
        "workspace".to_string(),
        Value::String(workspace.unwrap_or_default().to_string()),
    );
    result_obj.insert(
        "now".to_string(),
        json!({
            "headline": headline,
            "jobs": {
                "running": running,
                "queued": queued,
                "inbox": inbox_count,
                "critical": critical_regressions_count
            },
            "runner": {
                "status": runner_status_name,
                "runner_id": runner_status.get("runner_id").cloned().unwrap_or(Value::Null),
                "live_count": runner_status.get("live_count").cloned().unwrap_or(Value::Null),
                "idle_count": runner_status.get("idle_count").cloned().unwrap_or(Value::Null),
                "offline_count": runner_status.get("offline_count").cloned().unwrap_or(Value::Null)
            },
            "pipeline_health": pipeline_health
        }),
    );
    result_obj.insert(
        "proven".to_string(),
        json!({
            "guardrails": center_obj.get("defaults").cloned().unwrap_or(Value::Null),
            "execution_health": {
                "needs_manager": needs_manager,
                "needs_proof": needs_proof,
                "stalled_jobs": stalled_jobs
            },
            "pipeline_health": center_obj.get("pipeline_health").cloned().unwrap_or(Value::Null)
        }),
    );
    result_obj.insert(
        "critical_regressions_count".to_string(),
        json!(critical_regressions_count),
    );
    result_obj.insert(
        "critical_regressions".to_string(),
        Value::Array(critical_regressions),
    );
    result_obj.insert(
        "next".to_string(),
        Value::Array(actions.iter().take(3).map(compact_action).collect()),
    );
    result_obj.insert(
        "source".to_string(),
        Value::String("jobs.control.center".to_string()),
    );

    if include_details {
        result_obj.insert(
            "details".to_string(),
            json!({
                "scope": center_obj.get("scope").cloned().unwrap_or(Value::Null),
                "inbox": center_obj.get("inbox").cloned().unwrap_or(Value::Null),
                "execution_health": center_obj.get("execution_health").cloned().unwrap_or(Value::Null),
                "proof_health": center_obj.get("proof_health").cloned().unwrap_or(Value::Null),
                "pipeline_health": center_obj.get("pipeline_health").cloned().unwrap_or(Value::Null),
                "team_mesh": center_obj.get("team_mesh").cloned().unwrap_or(Value::Null),
            }),
        );
    }

    let mut resp = OpResponse::success(intent, Value::Object(result_obj));
    resp.warnings = center.warnings;
    resp.actions = actions;
    resp
}
