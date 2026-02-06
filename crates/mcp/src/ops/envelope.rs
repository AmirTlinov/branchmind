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
                "Use tools/list to see the strict v1 surface (10 portals). Then call tools/call with one of those names. Long-tail ops are available via op=\"call\" + cmd (see system ops.summary / schema.get)."
                    .to_string(),
            ),
        },
    );
    resp.actions.push(Action {
        action_id: format!("recover.system.ops.summary::{name}"),
        priority: ActionPriority::High,
        tool: ToolName::SystemOps.as_str().to_string(),
        args: json!({ "op": "ops.summary", "args": {}, "budget_profile": "portal", "view": "compact" }),
        why: "Показать v1 поверхность (10 порталов) + золотые ops + cmd registry summary.".to_string(),
        risk: "Низкий".to_string(),
    });
    resp.actions.push(Action {
        action_id: "recover.status.portal".to_string(),
        priority: ActionPriority::Medium,
        tool: ToolName::Status.as_str().to_string(),
        args: json!({ "budget_profile": "portal", "view": "compact" }),
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
        return OpResponse::error(
            "error".to_string(),
            OpError {
                code: "UNKNOWN_CMD".to_string(),
                message: format!("Unknown cmd: {}", env.cmd),
                recovery: Some("Use system op=schema.get to discover cmd schemas.".to_string()),
            },
        )
        .into_value();
    };

    if !spec.tier.allowed_in_toolset(server.toolset) {
        let mut resp = OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "CMD_NOT_AVAILABLE".to_string(),
                message: format!(
                    "cmd is not available in toolset={}",
                    server.toolset.as_str()
                ),
                recovery: Some(
                    "Use the kernel surface (gold ops) or restart the server with a larger toolset. Prefer tasks.execute.next for guided recovery."
                        .to_string(),
                ),
            },
        );

        if let Some(ws) = env.workspace.as_deref() {
            resp.actions.push(Action {
                action_id: "recover::tasks.execute.next".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::TasksOps.as_str().to_string(),
                args: json!({
                    "workspace": ws,
                    "op": "call",
                    "cmd": "tasks.execute.next",
                    "args": {},
                    "budget_profile": BudgetProfile::Portal.as_str(),
                    "view": "compact"
                }),
                why: "Получить следующий лучший шаг (kernel UX).".to_string(),
                risk: "Низкий".to_string(),
            });
        }

        resp.actions.push(Action {
            action_id: "recover::system.ops.summary".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::SystemOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "system.ops.summary",
                "args": {},
                "budget_profile": BudgetProfile::Portal.as_str(),
                "view": "compact"
            }),
            why: "Посмотреть минимальную сводку доступной поверхности (kernel).".to_string(),
            risk: "Низкий".to_string(),
        });

        return resp.into_value();
    }

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
        crate::ops::recovery::append_invalid_input_actions(
            &mut response,
            &env.cmd,
            env.workspace.as_deref(),
        );
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
    response.into_value()
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
            recovery: None,
        })?;
        if registry.find_by_cmd(&cmd).is_none() {
            return Err(OpError {
                code: "UNKNOWN_CMD".to_string(),
                message: format!("Unknown cmd: {cmd}"),
                recovery: Some("Use system op=schema.get to discover cmd schemas.".to_string()),
            });
        }
        cmd
    } else {
        let spec = registry.find_by_alias(tool, &op).ok_or_else(|| OpError {
            code: "UNKNOWN_OP".to_string(),
            message: format!("Unknown op: {op}"),
            recovery: Some("Use op=call + cmd or tools/list for golden ops.".to_string()),
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

    let view = args_obj
        .get("view")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let Some(view) = &view
        && !matches!(view.as_str(), "compact" | "smart" | "audit")
    {
        return Err(OpError {
            code: "INVALID_INPUT".to_string(),
            message: "view must be one of: compact|smart|audit".to_string(),
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
        apply_budget_caps(&mut args, caps)?;
    } else {
        // Unknown cmd already handled above.
    }

    Ok(Envelope {
        workspace,
        cmd,
        args,
    })
}

fn apply_budget_caps(args: &mut Value, caps: crate::ops::BudgetCaps) -> Result<(), OpError> {
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
        if let Some(v) = obj.get("limit").and_then(|v| v.as_u64()) {
            if v as usize > limit {
                return Err(OpError {
                    code: "BUDGET_EXCEEDED".to_string(),
                    message: "limit exceeds budget profile".to_string(),
                    recovery: Some(format!("Use limit <= {limit}")),
                });
            }
        } else {
            obj.insert("limit".to_string(), Value::Number(limit.into()));
        }
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
