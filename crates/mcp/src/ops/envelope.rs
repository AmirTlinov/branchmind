#![forbid(unsafe_code)]

use crate::McpServer;
use crate::WorkspaceId;
use crate::ops::{Action, ActionPriority, BudgetProfile, CommandRegistry, ToolName};
use crate::support::now_rfc3339;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(crate) struct Envelope {
    pub(crate) workspace: Option<String>,
    pub(crate) budget_profile: BudgetProfile,
    pub(crate) portal_view: Option<String>,
    pub(crate) cmd: String,
    pub(crate) args: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct OpError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) recovery: Option<String>,
}

impl OpError {
    pub(crate) fn to_value(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("code".to_string(), Value::String(self.code.clone()));
        obj.insert("message".to_string(), Value::String(self.message.clone()));
        if let Some(recovery) = &self.recovery {
            obj.insert("recovery".to_string(), Value::String(recovery.clone()));
        }
        Value::Object(obj)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OpResponse {
    pub(crate) intent: String,
    pub(crate) result: Value,
    pub(crate) refs: Vec<String>,
    pub(crate) warnings: Vec<Value>,
    pub(crate) actions: Vec<Action>,
    pub(crate) error: Option<OpError>,
}

impl OpResponse {
    pub(crate) fn success(intent: String, result: Value) -> Self {
        Self {
            intent,
            result,
            refs: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn error(intent: String, error: OpError) -> Self {
        Self {
            intent,
            result: json!({}),
            refs: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
            error: Some(error),
        }
    }

    pub(crate) fn into_value(mut self) -> Value {
        self.actions.sort_by(|a, b| {
            a.priority
                .rank()
                .cmp(&b.priority.rank())
                .then_with(|| a.action_id.cmp(&b.action_id))
        });
        json!({
            "success": self.error.is_none(),
            "intent": self.intent,
            "result": self.result,
            "refs": if self.refs.is_empty() { Value::Null } else { Value::Array(self.refs.into_iter().map(Value::String).collect()) },
            "actions": self.actions.iter().map(|a| a.to_json()).collect::<Vec<_>>(),
            "warnings": self.warnings,
            "suggestions": [],
            "context": {},
            "error": self.error.as_ref().map(|e| e.to_value()).unwrap_or(Value::Null),
            "timestamp": now_rfc3339(),
        })
    }
}

pub(crate) fn error_unknown_tool(name: &str) -> Value {
    let mut resp = OpResponse::error(
        "error".to_string(),
        OpError {
            code: "UNKNOWN_TOOL".to_string(),
            message: format!("Unknown tool: {name}"),
            recovery: Some(
                "Use system migration.lookup to map old names, or tools/list for v1 surface."
                    .to_string(),
            ),
        },
    );
    resp.actions.push(Action {
        action_id: format!("recover.migration.lookup::{name}"),
        priority: ActionPriority::High,
        tool: ToolName::SystemOps.as_str().to_string(),
        args: json!({ "op": "migration.lookup", "args": { "old_name": name }, "budget_profile": "portal" }),
        why: "Найти новый cmd для старого имени инструмента.".to_string(),
        risk: "Без миграции вызов будет UNKNOWN_TOOL.".to_string(),
    });
    resp.actions.push(Action {
        action_id: "recover.status.portal".to_string(),
        priority: ActionPriority::Medium,
        tool: ToolName::Status.as_str().to_string(),
        args: json!({ "budget_profile": "portal", "portal_view": "compact" }),
        why: "Открыть портал status (следующие действия + refs).".to_string(),
        risk: "Низкий".to_string(),
    });
    resp.into_value()
}

pub(crate) fn error_internal(message: String) -> Value {
    OpResponse::error(
        "error".to_string(),
        OpError {
            code: "INTERNAL_ERROR".to_string(),
            message,
            recovery: Some(
                "Retry the call. If it repeats, restart the server and capture logs.".to_string(),
            ),
        },
    )
    .into_value()
}

pub(crate) fn handle_ops_call(server: &mut McpServer, tool: ToolName, raw_args: Value) -> Value {
    let registry = CommandRegistry::global();
    let raw_args_for_err = raw_args.clone();
    let env = match parse_envelope(server, tool, raw_args, registry) {
        Ok(v) => v,
        Err(err) => {
            let mut resp = OpResponse::error("error".to_string(), err.clone());

            // UX: parse-time errors should still be actionable (schema-on-demand + safe retry).
            if let Some(args_obj) = raw_args_for_err.as_object() {
                append_parse_error_actions(&mut resp, tool, args_obj, &err, registry);

                let workspace = args_obj.get("workspace").and_then(|v| v.as_str());
                let cmd = cmd_for_error_recovery(tool, args_obj, registry);
                if let Some(cmd) = cmd.as_deref() {
                    if err.code == "INVALID_INPUT" {
                        crate::ops::append_schema_actions(&mut resp, cmd, workspace);
                    } else if err.code == "BUDGET_EXCEEDED" {
                        append_budget_exceeded_actions(&mut resp, tool, cmd, args_obj, registry);
                        crate::ops::append_schema_actions(&mut resp, cmd, workspace);
                    }
                }
            }

            return resp.into_value();
        }
    };
    let Some(spec) = registry.find_by_cmd(&env.cmd) else {
        let recovery = crate::ops::recovery::removed_knowledge_recovery(&env.cmd)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                "Use system op=cmd.list to discover cmds, then system op=schema.get for exact args."
                    .to_string()
            });
        return OpResponse::error(
            "error".to_string(),
            OpError {
                code: "UNKNOWN_CMD".to_string(),
                message: format!("Unknown cmd: {}", env.cmd),
                recovery: Some(recovery),
            },
        )
        .into_value();
    };

    let mut response = if spec.handler.is_some() {
        crate::ops::dispatch_custom(server, spec, &env)
    } else if let Some(handler_name) = &spec.handler_name {
        // v1 envelope keeps `workspace` outside of `args`, but handler contracts still expect it
        // inside the args object. To preserve compatibility (and enable default-workspace DX),
        // we inject workspace into handler args when missing.
        let mut handler_args = env.args.clone();
        if let Some(workspace) = env.workspace.as_deref()
            && let Some(obj) = handler_args.as_object_mut()
            && !obj.contains_key("workspace")
        {
            obj.insert(
                "workspace".to_string(),
                Value::String(workspace.to_string()),
            );
        }

        let handler_resp = crate::handlers::dispatch_handler(server, handler_name, handler_args)
            .unwrap_or_else(|| {
                OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INTERNAL_ERROR".to_string(),
                        message: format!("Handler dispatch failed for {handler_name}"),
                        recovery: Some("Check handler registry wiring for this cmd.".to_string()),
                    },
                )
                .into_value()
            });
        crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
    } else {
        OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INTERNAL_ERROR".to_string(),
                message: "No handler available for cmd".to_string(),
                recovery: Some("Check registry wiring.".to_string()),
            },
        )
    };
    if let Some(err) = response.error.as_ref()
        && err.code == "INVALID_INPUT"
    {
        crate::ops::append_schema_actions(&mut response, &env.cmd, env.workspace.as_deref());
    }
    if let Some(err) = response.error.as_ref()
        && err.code == "UNKNOWN_ID"
    {
        crate::ops::recovery::append_unknown_id_actions(
            &mut response,
            &env.cmd,
            env.workspace.as_deref(),
        );
    }
    append_budget_truncation_actions(&mut response, tool, &env);
    response.into_value()
}

fn append_budget_truncation_actions(resp: &mut OpResponse, tool: ToolName, env: &Envelope) {
    if resp.error.is_some() {
        return;
    }
    let truncated = resp.warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    });
    if !truncated {
        return;
    }

    let target_profile = match env.budget_profile {
        BudgetProfile::Portal => BudgetProfile::Default,
        BudgetProfile::Default => BudgetProfile::Audit,
        BudgetProfile::Audit => BudgetProfile::Audit,
    };

    let mut retry_args = env.args.as_object().cloned().unwrap_or_default();
    // Retry should be copy/paste-ready: drop explicit budget knobs so the selected budget_profile
    // can re-apply its defaults (including larger caps).
    retry_args.remove("max_chars");
    retry_args.remove("context_budget");

    // Hygiene: strip inner workspace duplication when we already have an outer workspace.
    if let Some(ws) = env.workspace.as_deref()
        && retry_args.get("workspace").and_then(|v| v.as_str()) == Some(ws)
    {
        retry_args.remove("workspace");
    }

    let mut retry_env = serde_json::Map::new();
    if let Some(ws) = env.workspace.as_deref() {
        retry_env.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    retry_env.insert("op".to_string(), Value::String("call".to_string()));
    retry_env.insert("cmd".to_string(), Value::String(env.cmd.clone()));
    retry_env.insert("args".to_string(), Value::Object(retry_args));
    retry_env.insert(
        "budget_profile".to_string(),
        Value::String(target_profile.as_str().to_string()),
    );
    retry_env.insert(
        "portal_view".to_string(),
        Value::String(env.portal_view.as_deref().unwrap_or("compact").to_string()),
    );

    // Avoid action spam.
    let action_id = format!("recover.budget.truncation::{}", env.cmd);
    if resp.actions.iter().any(|a| a.action_id == action_id) {
        return;
    }

    resp.actions.push(Action {
        action_id,
        priority: ActionPriority::High,
        tool: tool.as_str().to_string(),
        args: Value::Object(retry_env),
        why: "Повторить вызов с большим budget_profile (и без жёстких max_chars/context_budget)."
            .to_string(),
        risk: "Ответ может стать более объёмным; при необходимости сузьте limit/фильтры."
            .to_string(),
    });
}

fn suggest_cmd_list_query(cmd_raw: &str) -> String {
    let parts = cmd_raw.trim().to_ascii_lowercase();
    let parts = parts.split('.').collect::<Vec<_>>();
    // Heuristic: for `domain.category.verb` prefer searching by category, otherwise by verb.
    if parts.len() >= 3 {
        return parts[1].to_string();
    }
    parts.last().unwrap_or(&"").to_string()
}

fn append_parse_error_actions(
    resp: &mut OpResponse,
    tool: ToolName,
    args_obj: &serde_json::Map<String, Value>,
    err: &OpError,
    registry: &CommandRegistry,
) {
    let mut seen = BTreeSet::<String>::new();
    for a in resp.actions.iter() {
        seen.insert(a.action_id.clone());
    }

    if err.code == "UNKNOWN_OP" {
        // Common DX footgun: op=<fully-qualified cmd> (e.g. op="tasks.snapshot").
        // Suggest retrying as op=call + cmd=<op> (and preserve portal_view vs cmd-args.view).
        if let Some(op_raw) = args_obj.get("op").and_then(|v| v.as_str())
            && let Ok(cmd_candidate) = crate::ops::normalize_cmd(op_raw)
            && let Some(spec) = registry.find_by_cmd(&cmd_candidate)
            && spec.domain_tool == tool
        {
            let mut retry_env = args_obj.clone();
            retry_env.insert("op".to_string(), Value::String("call".to_string()));
            retry_env.insert("cmd".to_string(), Value::String(cmd_candidate.clone()));

            // Ensure args is an object (op=call requires it).
            let args_entry = retry_env
                .entry("args".to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if !args_entry.is_object() {
                *args_entry = Value::Object(serde_json::Map::new());
            }

            // If both portal_view and view are set and differ, treat `view` as the cmd-arg
            // (common in legacy portal calls) and move it under args.view.
            let portal_view = retry_env
                .get("portal_view")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let view = retry_env
                .get("view")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let (Some(portal_view), Some(view)) = (portal_view, view)
                && portal_view != view
            {
                if let Some(inner) = retry_env.get_mut("args").and_then(|v| v.as_object_mut()) {
                    inner
                        .entry("view".to_string())
                        .or_insert_with(|| Value::String(view.to_string()));
                }
                retry_env.remove("view");
            }

            let action_id = format!("recover.did_you_mean.call::{cmd_candidate}");
            if seen.insert(action_id.clone()) {
                resp.actions.push(Action {
                    action_id,
                    priority: ActionPriority::High,
                    tool: tool.as_str().to_string(),
                    args: Value::Object(retry_env),
                    why: format!(
                        "op выглядит как cmd; повторить вызов как op=call cmd={cmd_candidate}."
                    ),
                    risk: "Низкий".to_string(),
                });
            }
        }

        let action_id = "recover.system.ops.summary".to_string();
        if seen.insert(action_id.clone()) {
            resp.actions.push(Action {
                action_id,
                priority: ActionPriority::High,
                tool: "system".to_string(),
                args: json!({
                    "op": "ops.summary",
                    "args": {},
                    "budget_profile": "portal"
                }),
                why: "Показать доступные ops/cmd (SSOT) и быстро найти правильный вызов."
                    .to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }

    let Some(cmd_raw) = args_obj.get("cmd").and_then(|v| v.as_str()) else {
        return;
    };
    let cmd_raw = cmd_raw.trim();
    if cmd_raw.is_empty() {
        return;
    }

    let cmd_list_q = suggest_cmd_list_query(cmd_raw);

    // UNKNOWN_CMD: most common DX footgun is calling op=call with cmd=<op-alias>
    // (e.g. cmd="schema.get"). Suggest retrying as op=<alias> (omit cmd) when possible.
    if err.code == "UNKNOWN_CMD" {
        if let Ok(alias) = crate::ops::normalize_op(cmd_raw)
            && alias != "call"
            && registry.find_by_alias(tool, &alias).is_some()
        {
            let mut retry_env = args_obj.clone();
            retry_env.insert("op".to_string(), Value::String(alias.clone()));
            retry_env.remove("cmd");

            let action_id = format!("recover.did_you_mean.op::{alias}");
            if seen.insert(action_id.clone()) {
                resp.actions.push(Action {
                    action_id,
                    priority: ActionPriority::High,
                    tool: tool.as_str().to_string(),
                    args: Value::Object(retry_env),
                    why: format!(
                        "cmd похож на op-алиас; повторить вызов как op={alias} (без cmd)."
                    ),
                    risk: "Низкий".to_string(),
                });
            }

            // Keep a discovery fallback as the second action.
            let action_id = "recover.system.cmd.list".to_string();
            if seen.insert(action_id.clone()) {
                resp.actions.push(Action {
                    action_id,
                    priority: ActionPriority::Medium,
                    tool: "system".to_string(),
                    args: json!({
                        "op": "cmd.list",
                        "args": { "q": cmd_list_q },
                        "budget_profile": "portal"
                    }),
                    why: "Найти правильный cmd (поиск по подстроке).".to_string(),
                    risk: "Низкий".to_string(),
                });
            }
            return;
        }

        let action_id = "recover.system.cmd.list".to_string();
        if seen.insert(action_id.clone()) {
            resp.actions.push(Action {
                action_id,
                priority: ActionPriority::High,
                tool: "system".to_string(),
                args: json!({
                    "op": "cmd.list",
                    "args": { "q": cmd_list_q },
                    "budget_profile": "portal"
                }),
                why: "Найти правильный cmd (поиск по подстроке).".to_string(),
                risk: "Низкий".to_string(),
            });
        }
        return;
    }

    // INVALID_INPUT where cmd is malformed: provide a deterministic legacy escape hatch.
    if err.code == "INVALID_INPUT" && err.message.starts_with("cmd ") {
        let legacy_candidate = if !cmd_raw.contains('.') && cmd_raw.contains('_') {
            let candidate = crate::ops::name_to_cmd_segments(cmd_raw);
            if registry.find_by_cmd(&candidate).is_some() {
                Some(candidate)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(candidate) = legacy_candidate {
            let mut retry_env = args_obj.clone();
            retry_env.insert("op".to_string(), Value::String("call".to_string()));
            retry_env.insert("cmd".to_string(), Value::String(candidate.clone()));
            if !retry_env.contains_key("args") {
                retry_env.insert("args".to_string(), Value::Object(serde_json::Map::new()));
            }

            let action_id = format!("recover.legacy.cmd.to_dotted::{candidate}");
            if seen.insert(action_id.clone()) {
                resp.actions.push(Action {
                    action_id,
                    priority: ActionPriority::High,
                    tool: tool.as_str().to_string(),
                    args: Value::Object(retry_env),
                    why: "Похоже на legacy cmd (snake_case). Повторить с точками вместо '_'."
                        .to_string(),
                    risk: "Низкий".to_string(),
                });
            }

            let action_id = "recover.system.migration.lookup".to_string();
            if seen.insert(action_id.clone()) {
                resp.actions.push(Action {
                    action_id,
                    priority: ActionPriority::Medium,
                    tool: "system".to_string(),
                    args: json!({
                        "op": "migration.lookup",
                        "args": { "old_name": cmd_raw },
                        "budget_profile": "portal"
                    }),
                    why: "Маппинг legacy имени → v1 cmd (с примером вызова).".to_string(),
                    risk: "Низкий".to_string(),
                });
            }
            return;
        }

        let action_id = "recover.system.migration.lookup".to_string();
        if seen.insert(action_id.clone()) {
            resp.actions.push(Action {
                action_id,
                priority: ActionPriority::High,
                tool: "system".to_string(),
                args: json!({
                    "op": "migration.lookup",
                    "args": { "old_name": cmd_raw },
                    "budget_profile": "portal"
                }),
                why: "Маппинг legacy имени → v1 cmd (с примером вызова).".to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }
}

fn cmd_for_error_recovery(
    tool: ToolName,
    args_obj: &serde_json::Map<String, Value>,
    registry: &CommandRegistry,
) -> Option<String> {
    let op = args_obj.get("op").and_then(|v| v.as_str())?;
    if op == "call" {
        let cmd_raw = args_obj.get("cmd").and_then(|v| v.as_str())?;
        let cmd = crate::ops::normalize_cmd(cmd_raw).ok()?;
        if registry.find_by_cmd(&cmd).is_some() {
            Some(cmd)
        } else {
            None
        }
    } else {
        registry
            .find_by_alias(tool, op)
            .map(|spec| spec.cmd.clone())
    }
}

fn append_budget_exceeded_actions(
    resp: &mut OpResponse,
    tool: ToolName,
    cmd: &str,
    args_obj: &serde_json::Map<String, Value>,
    registry: &CommandRegistry,
) {
    let Some(spec) = registry.find_by_cmd(cmd) else {
        return;
    };

    let budget_profile = args_obj
        .get("budget_profile")
        .and_then(|v| v.as_str())
        .and_then(BudgetProfile::from_str)
        .unwrap_or(spec.budget.default_profile);
    let caps = spec.budget.caps_for(budget_profile);

    let mut retry_env = args_obj.clone();
    retry_env.insert(
        "budget_profile".to_string(),
        Value::String(budget_profile.as_str().to_string()),
    );

    let Some(retry_args_obj) = retry_env.get_mut("args").and_then(|v| v.as_object_mut()) else {
        return;
    };

    let mut clamped = false;
    if let Some(max_chars) = caps.max_chars
        && let Some(v) = retry_args_obj.get("max_chars").and_then(|v| v.as_u64())
        && v as usize > max_chars
    {
        retry_args_obj.insert("max_chars".to_string(), Value::Number(max_chars.into()));
        clamped = true;
    }
    if let Some(context_budget) = caps.context_budget
        && let Some(v) = retry_args_obj
            .get("context_budget")
            .and_then(|v| v.as_u64())
        && v as usize > context_budget
    {
        retry_args_obj.insert(
            "context_budget".to_string(),
            Value::Number(context_budget.into()),
        );
        clamped = true;
    }
    if let Some(limit) = caps.limit
        && let Some(v) = retry_args_obj.get("limit").and_then(|v| v.as_u64())
        && v as usize > limit
    {
        retry_args_obj.insert("limit".to_string(), Value::Number(limit.into()));
        clamped = true;
    }
    if !clamped {
        // If we can't deterministically identify the offending knob, don't emit a misleading retry.
        return;
    }

    let mut seen = BTreeSet::<String>::new();
    for a in resp.actions.iter() {
        seen.insert(a.action_id.clone());
    }

    let action_id = format!("recover.budget.clamp::{cmd}");
    if !seen.insert(action_id.clone()) {
        return;
    }

    resp.actions.push(Action {
        action_id,
        priority: ActionPriority::High,
        tool: tool.as_str().to_string(),
        args: Value::Object(retry_env),
        why: "Retry with budget-safe caps (auto-clamped to the selected budget profile)."
            .to_string(),
        risk: "Output may be truncated; consider switching to a larger budget_profile when needed."
            .to_string(),
    });
}

fn parse_envelope(
    server: &mut McpServer,
    tool: ToolName,
    raw_args: Value,
    registry: &CommandRegistry,
) -> Result<Envelope, OpError> {
    let Some(args_obj) = raw_args.as_object() else {
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: "arguments must be an object".to_string(),
            recovery: Some("Provide a JSON object with op/cmd/args.".to_string()),
        });
    };

    let op_raw = args_obj
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OpError {
            code: "INVALID_INPUT".to_string(),
            message: "op is required".to_string(),
            recovery: Some("Provide op (or op=call + cmd).".to_string()),
        })?;
    let op = crate::ops::normalize_op(op_raw).map_err(|msg| OpError {
        code: "INVALID_INPUT".to_string(),
        message: format!("op {msg}"),
        recovery: None,
    })?;

    let cmd = if op == "call" {
        let cmd_raw = args_obj
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpError {
                code: "INVALID_INPUT".to_string(),
                message: "cmd is required for op=call".to_string(),
                recovery: Some("Provide cmd with op=call.".to_string()),
            })?;
        let cmd = crate::ops::normalize_cmd(cmd_raw).map_err(|msg| OpError {
            code: "INVALID_INPUT".to_string(),
            message: format!("cmd {msg}"),
            recovery: Some(
                "cmd must be fully-qualified (e.g. tasks.snapshot). If you meant a golden op alias (plan.create), use op=plan.create and omit cmd; for legacy names use system op=migration.lookup."
                    .to_string(),
            ),
        })?;
        if registry.find_by_cmd(&cmd).is_some() {
            cmd
        } else {
            let legacy_candidate = cmd.replace('_', ".");
            if legacy_candidate != cmd && registry.find_by_cmd(&legacy_candidate).is_some() {
                legacy_candidate
            } else {
                let recovery = crate::ops::recovery::removed_knowledge_recovery(&cmd)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        "Use system op=cmd.list (q/prefix) to discover cmds, then system op=schema.get for exact args. If cmd looks like an op alias, use op=<alias> and omit cmd."
                            .to_string()
                    });
                return Err(OpError {
                    code: "UNKNOWN_CMD".to_string(),
                    message: format!("Unknown cmd: {cmd}"),
                    recovery: Some(recovery),
                });
            }
        }
    } else {
        let spec = registry.find_by_alias(tool, &op).ok_or_else(|| OpError {
            code: "UNKNOWN_OP".to_string(),
            message: format!("Unknown op: {op}"),
            recovery: Some(
                "Use system op=ops.summary to see available ops, or use op=call + cmd (discover via system op=cmd.list)."
                    .to_string(),
            ),
        })?;
        spec.cmd.clone()
    };

    let args_value = args_obj.get("args").cloned().unwrap_or(Value::Null);
    let args_obj_inner = match args_value {
        Value::Null => serde_json::Map::new(),
        Value::Object(map) => map,
        _ => {
            return Err(OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some(
                    "Provide args as a JSON object (or null/missing for empty).".to_string(),
                ),
            });
        }
    };

    let portal_view_raw = args_obj.get("portal_view").and_then(|v| v.as_str());
    let legacy_view_raw = args_obj.get("view").and_then(|v| v.as_str());
    let view = match (portal_view_raw, legacy_view_raw) {
        (Some(portal_view), Some(view)) => {
            if !portal_view.trim().eq_ignore_ascii_case(view.trim()) {
                return Err(OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "provide portal_view or view, not both".to_string(),
                    recovery: Some(
                        "Use portal_view for envelope response shaping; cmd-specific view remains inside args."
                            .to_string(),
                    ),
                });
            }
            Some(portal_view.to_string())
        }
        (Some(portal_view), None) => Some(portal_view.to_string()),
        (None, Some(view)) => Some(view.to_string()),
        (None, None) => None,
    };
    if let Some(view) = &view
        && !matches!(view.trim(), "compact" | "smart" | "audit")
    {
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: "portal_view must be one of: compact|smart|audit".to_string(),
            recovery: None,
        });
    }

    let budget_profile_explicit = args_obj.get("budget_profile").and_then(|v| v.as_str());
    let mut budget_profile = budget_profile_explicit
        .and_then(BudgetProfile::from_str)
        .unwrap_or(BudgetProfile::Default);

    let mut workspace = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if workspace.is_none() {
        workspace = args_obj_inner
            .get("workspace")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    if workspace.is_none() {
        workspace = server
            .workspace_override
            .clone()
            .or_else(|| server.default_workspace.clone());
    }

    // DX: accept workspace as a filesystem path (e.g. "/home/me/repo") and resolve/bind it to a
    // stable WorkspaceId before allowlist/lock checks run.
    if let Some(ws) = workspace.as_deref() {
        match server.workspace_id_resolve(ws) {
            Ok(resolved) => workspace = Some(resolved),
            Err(crate::StoreError::InvalidInput(msg)) => {
                return Err(OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: Some(
                        "Use workspace like my-workspace (or pass a repo path to auto-bind)."
                            .to_string(),
                    ),
                });
            }
            Err(err) => {
                return Err(OpError {
                    code: "STORE_ERROR".to_string(),
                    message: crate::format_store_error(err),
                    recovery: None,
                });
            }
        }
    }

    let mut args = Value::Object(args_obj_inner.clone());
    if cmd == "tasks.resume.super"
        && let Some(obj) = args.as_object_mut()
        && obj.contains_key("target")
        && !obj.contains_key("task")
        && !obj.contains_key("plan")
    {
        if let Some(target) = obj.get("target").and_then(|v| v.as_str()) {
            let key = if target.starts_with("PLAN-") {
                "plan"
            } else {
                "task"
            };
            obj.insert(key.to_string(), Value::String(target.to_string()));
        }
        obj.remove("target");
    }
    if let Some(ws) = workspace.as_ref() {
        let args_obj_mut = args.as_object_mut().expect("args object");
        if !args_obj_mut.contains_key("workspace") {
            args_obj_mut.insert("workspace".to_string(), Value::String(ws.clone()));
        }
    }

    if let Some(ws) = workspace.as_ref() {
        enforce_workspace_policy(server, ws)?;
        ensure_workspace_initialized(server, ws)?;
    }

    if tool == ToolName::TasksOps
        && let Some(default_agent_id) = server.default_agent_id.as_deref()
    {
        let args_obj_mut = args.as_object_mut().expect("args object");
        if !args_obj_mut.contains_key("agent_id") {
            args_obj_mut.insert(
                "agent_id".to_string(),
                Value::String(default_agent_id.to_string()),
            );
        }
    }

    if let Some(spec) = registry.find_by_cmd(&cmd) {
        if budget_profile_explicit.is_none() {
            budget_profile = spec.budget.default_profile;
        }
        let caps = spec.budget.caps_for(budget_profile);
        apply_budget_caps(&mut args, &cmd, caps)?;
    } else {
        // Unknown cmd already handled above.
    }

    Ok(Envelope {
        workspace,
        budget_profile,
        portal_view: view,
        cmd,
        args,
    })
}

fn apply_budget_caps(
    args: &mut Value,
    cmd: &str,
    caps: crate::ops::BudgetCaps,
) -> Result<(), OpError> {
    let Some(obj) = args.as_object_mut() else {
        return Ok(());
    };
    if let Some(max_chars) = caps.max_chars {
        if let Some(v) = obj.get("max_chars").and_then(|v| v.as_u64()) {
            if v as usize > max_chars {
                return Err(OpError {
                    code: "BUDGET_EXCEEDED".to_string(),
                    message: "max_chars exceeds budget profile".to_string(),
                    recovery: Some(format!("Use max_chars <= {max_chars}")),
                });
            }
        } else {
            obj.insert("max_chars".to_string(), Value::Number(max_chars.into()));
        }
    }
    if let Some(context_budget) = caps.context_budget {
        if let Some(v) = obj.get("context_budget").and_then(|v| v.as_u64()) {
            if v as usize > context_budget {
                return Err(OpError {
                    code: "BUDGET_EXCEEDED".to_string(),
                    message: "context_budget exceeds budget profile".to_string(),
                    recovery: Some(format!("Use context_budget <= {context_budget}")),
                });
            }
        } else {
            obj.insert(
                "context_budget".to_string(),
                Value::Number(context_budget.into()),
            );
        }
    }
    if let Some(limit) = caps.limit {
        if let Some(v) = obj.get("limit").and_then(|v| v.as_u64())
            && v as usize > limit
        {
            return Err(OpError {
                code: "BUDGET_EXCEEDED".to_string(),
                message: "limit exceeds budget profile".to_string(),
                recovery: Some(format!("Use limit <= {limit}")),
            });
        }
        // Intentionally do NOT inject default `limit` from budget profile.
        //
        // `limit` is a semantic argument for many commands (selection scan depth, result
        // cardinality, etc). Auto-injecting profile limits silently overrides command-level
        // defaults (e.g. macro auto-select defaults) and regresses flagship UX predictability.
        //
        // We still enforce an upper cap when users explicitly pass `limit`.
        let _ = cmd; // keep signature explicit for future cmd-specific cap policies.
    }
    Ok(())
}

fn enforce_workspace_policy(server: &McpServer, workspace: &str) -> Result<(), OpError> {
    if server.workspace_lock
        && let Some(default_workspace) = server.default_workspace.as_deref()
        && workspace != default_workspace
    {
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: "workspace is locked to the configured default workspace".to_string(),
            recovery: Some("Drop workspace or restart without workspace lock.".to_string()),
        });
    }

    if let Some(allowlist) = server.workspace_allowlist.as_ref()
        && !allowlist.iter().any(|allowed| allowed == workspace)
    {
        let preview = allowlist
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: "workspace is not in allowlist".to_string(),
            recovery: Some(format!("Allowed workspaces (partial): {preview}")),
        });
    }
    Ok(())
}

fn ensure_workspace_initialized(server: &mut McpServer, workspace: &str) -> Result<(), OpError> {
    let workspace_id = WorkspaceId::try_new(workspace.to_string()).map_err(|_| OpError {
        code: "INVALID_INPUT".to_string(),
        message: "workspace: expected WorkspaceId".to_string(),
        recovery: Some("Use workspace like my-workspace".to_string()),
    })?;

    let exists = server
        .store
        .workspace_exists(&workspace_id)
        .map_err(|err| OpError {
            code: "INTERNAL_ERROR".to_string(),
            message: format!("store error: {err}"),
            recovery: None,
        })?;
    if !exists {
        server
            .store
            .workspace_init(&workspace_id)
            .map_err(|err| OpError {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("store error: {err}"),
                recovery: None,
            })?;
    }

    if let Some(err_resp) = server.enforce_project_guard(&workspace_id) {
        let code = err_resp
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("INTERNAL_ERROR")
            .to_string();
        let msg = err_resp
            .get("error")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("project guard mismatch")
            .to_string();
        return Err(OpError {
            code: if code == "INVALID_INPUT" {
                "INVALID_INPUT".to_string()
            } else {
                "INTERNAL_ERROR".to_string()
            },
            message: msg,
            recovery: None,
        });
    }

    Ok(())
}
