#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::{Value, json};
use std::collections::HashSet;

impl McpServer {
    pub(crate) fn new(
        store: bm_storage::SqliteStore,
        toolset: crate::Toolset,
        default_workspace: Option<String>,
        workspace_lock: bool,
        project_guard: Option<String>,
        default_agent_id: Option<String>,
    ) -> Self {
        Self {
            initialized: false,
            store,
            toolset,
            default_workspace,
            workspace_lock,
            project_guard,
            default_agent_id,
        }
    }

    pub(crate) fn handle(&mut self, request: crate::JsonRpcRequest) -> Option<Value> {
        let method = request.method.as_str();

        if method == "initialize" {
            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "protocolVersion": crate::MCP_VERSION,
                    "serverInfo": { "name": crate::SERVER_NAME, "version": crate::SERVER_VERSION },
                    "capabilities": { "tools": {} }
                }),
            ));
        }

        if !self.initialized && method != "notifications/initialized" {
            return Some(crate::json_rpc_error(
                request.id,
                -32002,
                "Server not initialized",
            ));
        }

        if method == "notifications/initialized" {
            self.initialized = true;
            return None;
        }

        if method == "ping" {
            return Some(crate::json_rpc_response(request.id, json!({})));
        }

        // MCP polish: some clients probe optional resources methods by default. We keep the
        // surface deterministic and minimal by advertising an empty resource set.
        if method == "resources/list" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "resources": [] }),
            ));
        }
        if method == "resources/read" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "contents": [] }),
            ));
        }

        if method == "tools/list" {
            let toolset = match request
                .params
                .as_ref()
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get("toolset"))
            {
                Some(v) => {
                    let Some(label) = v.as_str() else {
                        return Some(crate::json_rpc_error(
                            request.id,
                            -32602,
                            "toolset must be a string",
                        ));
                    };
                    match crate::Toolset::from_str(label) {
                        Some(v) => v,
                        None => {
                            return Some(crate::json_rpc_error(
                                request.id,
                                -32602,
                                "toolset must be one of: full|daily|core",
                            ));
                        }
                    }
                }
                None => self.toolset,
            };
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "tools": crate::tools::tool_definitions(toolset) }),
            ));
        }

        if method == "tools/call" {
            let Some(params) = request.params else {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };
            let Some(params_obj) = params.as_object() else {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };

            let tool_name = params_obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params_obj
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response_body = self.call_tool(tool_name, args);

            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "content": [crate::tool_text_content(&response_body)],
                    "isError": !response_body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                }),
            ));
        }

        Some(crate::json_rpc_error(
            request.id,
            -32601,
            &format!("Method not found: {method}"),
        ))
    }

    pub(crate) fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let mut args = args;
        let original_args = args.clone();
        if let Some(mut resp) = self.preprocess_args(name, &mut args) {
            // Even when we short-circuit during preprocessing (e.g. target normalization errors),
            // we still want the same portal-first recovery UX and the same output formatting.
            self.postprocess_response(name, &args, &mut resp);
            return resp;
        }
        let Some(mut resp) = crate::tools::dispatch_tool(self, name, args.clone()) else {
            let mut resp = crate::ai_error("UNKNOWN_TOOL", &format!("Unknown tool: {name}"));
            self.postprocess_response(name, &args, &mut resp);
            return resp;
        };
        if let Some((escalated_args, mut escalated_resp)) =
            self.auto_escalate_budget_if_needed(name, &original_args, &args, &resp)
        {
            self.postprocess_response(name, &escalated_args, &mut escalated_resp);
            return escalated_resp;
        }
        self.postprocess_response(name, &args, &mut resp);
        resp
    }

    fn auto_escalate_budget_if_needed(
        &mut self,
        name: &str,
        original_args: &Value,
        args: &Value,
        resp: &Value,
    ) -> Option<(Value, Value)> {
        // Safety: never rerun portal tools (fmt=lines + portal UX) and never override explicit budgets.
        if is_portal_tool(name) {
            return None;
        }

        let Some(original_obj) = original_args.as_object() else {
            return None;
        };
        if original_obj.contains_key("max_chars") || original_obj.contains_key("context_budget") {
            return None;
        }

        if resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return None;
        }
        if !response_has_budget_truncation_warning(resp) {
            return None;
        }

        // Only retry tools that are read-ish and safe to rerun. Even if some of these perform
        // internal idempotent "ensure" writes (workspace/doc/ref), they must not append history.
        if !auto_budget_escalation_allowlist(name) {
            return None;
        }

        // Goal: remove "limit juggling" friction while keeping outputs bounded and deterministic.
        // We retry a small, fixed number of times and stop early once truncation disappears.
        //
        // Important: this must remain safe for tools that are internally "read-ish" but may
        // perform idempotent ensure-writes (e.g. workspace/doc refs). They must never append
        // user-visible history on reads.
        const MAX_RETRIES: usize = 3;
        let cap = auto_budget_escalation_cap_chars(name);

        let mut current_args = args.clone();
        let mut current_resp = resp.clone();
        let mut did_escalate = false;

        for _ in 0..MAX_RETRIES {
            if !response_has_budget_truncation_warning(&current_resp) {
                break;
            }
            let Some((current_max_chars, used_chars)) = extract_budget_snapshot(&current_resp)
            else {
                break;
            };
            if current_max_chars >= cap {
                break;
            }

            let used_chars = used_chars.unwrap_or(current_max_chars);
            let mut next_max_chars = current_max_chars
                .saturating_mul(2)
                .max(used_chars.saturating_mul(2))
                .max(current_max_chars.saturating_add(1));
            if next_max_chars > cap {
                next_max_chars = cap;
            }
            if next_max_chars <= current_max_chars {
                break;
            }

            let mut next_args = current_args.clone();
            let Some(args_obj) = next_args.as_object_mut() else {
                break;
            };
            if crate::is_lines_fmt(args_obj.get("fmt").and_then(|v| v.as_str())) {
                break;
            }
            apply_auto_escalated_budget(args_obj, next_max_chars);

            let Some(next_resp) = crate::tools::dispatch_tool(self, name, next_args.clone()) else {
                break;
            };
            if next_resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
                break;
            }

            current_args = next_args;
            current_resp = next_resp;
            did_escalate = true;
        }

        if did_escalate {
            Some((current_args, current_resp))
        } else {
            None
        }
    }

    fn preprocess_args(&mut self, name: &str, args: &mut Value) -> Option<Value> {
        let args_obj = args.as_object_mut()?;

        if let Some(default_workspace) = self.default_workspace.as_deref()
            && !args_obj.contains_key("workspace")
            && is_portal_tool(name)
        {
            args_obj.insert(
                "workspace".to_string(),
                Value::String(default_workspace.to_string()),
            );
        }

        // Multi-agent DX: when a default agent_id is configured, implicitly apply it to tool calls
        // unless the caller explicitly provides agent_id (including null to disable).
        if let Some(default_agent_id) = self.default_agent_id.as_deref()
            && !args_obj.contains_key("agent_id")
        {
            args_obj.insert(
                "agent_id".to_string(),
                Value::String(default_agent_id.to_string()),
            );
        }

        // Anti-drift: when workspace lock is enabled, reject any explicit workspace that differs
        // from the configured default workspace. This prevents accidental cross-project reads/writes.
        if self.workspace_lock
            && let Some(default_workspace) = self.default_workspace.as_deref()
            && let Some(workspace) = args_obj.get("workspace").and_then(|v| v.as_str())
            && workspace != default_workspace
        {
            return Some(crate::ai_error_with(
                "WORKSPACE_LOCKED",
                "workspace is locked to the configured default workspace",
                Some(
                    "Drop the workspace argument (use the default) or restart the server without workspace lock.",
                ),
                vec![crate::suggest_call(
                    name,
                    "Retry using the default workspace (omit workspace).",
                    "high",
                    json!({}),
                )],
            ));
        }

        // AI-first invariant: portal tools are always context-first (BM-L1 lines).
        // Do not expose / depend on a json-vs-lines toggle in portals.
        if is_portal_tool(name) {
            args_obj.insert("fmt".to_string(), Value::String("lines".to_string()));
        }

        // Portal DX: keep the “advance progress” macro call nearly zero-syntax.
        // If checkpoints are omitted, default to "gate" (still enforces discipline).
        let checkpoints_missing = args_obj
            .get("checkpoints")
            .map(|v| v.is_null())
            .unwrap_or(true);
        if name == "tasks_macro_close_step" && checkpoints_missing {
            args_obj.insert("checkpoints".to_string(), Value::String("gate".to_string()));
        }

        // Portal UX: in reduced toolsets, default JSON outputs to bounded payloads so agents don't
        // need to keep repeating max_chars/resume_max_chars on every call. (fmt=lines replaces the
        // JSON payload entirely, so budgets are opt-in there.)
        if self.toolset != crate::Toolset::Full && is_portal_tool(name) {
            apply_portal_default_budgets(self.toolset, name, args_obj);
        }

        // Full toolset DX: for heavy read tools, apply deterministic default budgets when the
        // caller didn't opt into explicit max_chars/context_budget. This prevents accidental
        // context blowups while keeping callers fully in control once they specify budgets.
        if !is_portal_tool(name) {
            apply_read_tool_default_budgets(name, args_obj);
        }
        if self.default_workspace.is_none()
            && is_portal_tool(name)
            && !args_obj.contains_key("workspace")
        {
            return Some(crate::ai_error_with(
                "INVALID_INPUT",
                "workspace is required",
                Some(
                    "Configure a default workspace via --workspace / BRANCHMIND_WORKSPACE, or pass workspace explicitly.",
                ),
                Vec::new(),
            ));
        }

        if let Some(resp) = self.auto_init_workspace(args_obj) {
            return Some(resp);
        }
        if let Err(resp) = crate::normalize_target_map(name, args_obj) {
            return Some(resp);
        }
        None
    }

    fn postprocess_response(&self, tool: &str, args: &Value, response: &mut Value) {
        let fmt = args.get("fmt").and_then(|v| v.as_str());
        let wants_lines = crate::is_lines_fmt(fmt);

        let Some(resp_obj) = response.as_object_mut() else {
            return;
        };

        if !wants_lines && self.toolset == crate::Toolset::Full {
            inject_smart_navigation_suggestions(tool, args, resp_obj);
            return;
        }

        if self.toolset != crate::Toolset::Full {
            let error_code = resp_obj
                .get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let error_message = resp_obj
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let Some(suggestions) = resp_obj
                .get_mut("suggestions")
                .and_then(|v| v.as_array_mut())
            else {
                return;
            };
            if suggestions.is_empty() {
                // Portal-first recovery UX: even when a tool returns a typed error without
                // suggestions, provide at most 1–2 low-noise portal recovery commands.
                inject_portal_recovery_for_error(
                    tool,
                    args,
                    error_code.as_deref(),
                    error_message.as_deref(),
                    suggestions,
                    self.default_workspace.as_deref(),
                );
            }
            if !suggestions.is_empty() {
                let advertised = advertised_tool_names(self.toolset);
                let core_tools = advertised_tool_names(crate::Toolset::Core);
                let daily_tools = advertised_tool_names(crate::Toolset::Daily);

                let mut rebuilt = Vec::with_capacity(suggestions.len());
                let mut hidden_targets = Vec::new();

                for suggestion in suggestions.iter() {
                    let action = suggestion.get("action").and_then(|v| v.as_str());
                    if action == Some("call_tool") {
                        let target = suggestion
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !target.is_empty() && !advertised.contains(target) {
                            let params = suggestion.get("params").cloned().unwrap_or(Value::Null);
                            if let Some(portal) = portal_recovery_suggestion(
                                target,
                                &params,
                                tool,
                                args,
                                error_code.as_deref(),
                                self.default_workspace.as_deref(),
                            ) {
                                if let Some(portal_target) =
                                    portal.get("target").and_then(|v| v.as_str())
                                    && !portal_target.is_empty()
                                    && !advertised.contains(portal_target)
                                {
                                    hidden_targets.push(portal_target.to_string());
                                }
                                rebuilt.push(portal);
                            } else {
                                hidden_targets.push(target.to_string());
                                rebuilt.push(suggestion.clone());
                            }
                            continue;
                        }
                    }

                    rebuilt.push(suggestion.clone());
                }

                if let Some(escalation_toolset) =
                    escalation_toolset_for_hidden(&hidden_targets, &core_tools, &daily_tools)
                {
                    let already_has_disclosure = rebuilt.iter().any(|s| {
                        s.get("action").and_then(|v| v.as_str()) == Some("call_method")
                            && s.get("method").and_then(|v| v.as_str()) == Some("tools/list")
                    });
                    if !already_has_disclosure {
                        rebuilt.insert(
                            0,
                            crate::suggest_method(
                                "tools/list",
                                "Reveal the next toolset tier for recovery.",
                                "high",
                                json!({ "toolset": escalation_toolset }),
                            ),
                        );
                    }
                }

                let mut seen = HashSet::new();
                rebuilt.retain(|s| match serde_json::to_string(s) {
                    Ok(key) => seen.insert(key),
                    Err(_) => true,
                });

                suggestions.clear();
                suggestions.extend(rebuilt);
            }
        }

        if wants_lines {
            let omit_workspace = self.default_workspace.as_deref().is_some_and(|default_ws| {
                args.get("workspace")
                    .and_then(|v| v.as_str())
                    .is_some_and(|ws| ws == default_ws)
            });
            crate::apply_portal_line_format(tool, args, response, self.toolset, omit_workspace);
        }
    }

    fn auto_init_workspace(&mut self, args: &serde_json::Map<String, Value>) -> Option<Value> {
        let workspace_raw = args.get("workspace").and_then(|v| v.as_str())?;
        let workspace = match crate::WorkspaceId::try_new(workspace_raw.to_string()) {
            Ok(v) => v,
            Err(_) => {
                return Some(crate::ai_error(
                    "INVALID_INPUT",
                    "workspace: expected WorkspaceId; fix: workspace=\"my-workspace\"",
                ));
            }
        };
        match self.store.workspace_exists(&workspace) {
            Ok(true) => {
                let checkout = self.store.branch_checkout_get(&workspace);
                if matches!(checkout, Ok(None))
                    && let Err(err) = self.store.workspace_init(&workspace)
                {
                    return Some(crate::ai_error(
                        "STORE_ERROR",
                        &crate::format_store_error(err),
                    ));
                }
                if let Some(resp) = self.enforce_project_guard(&workspace) {
                    return Some(resp);
                }
                None
            }
            Ok(false) => match self.store.workspace_init(&workspace) {
                Ok(()) => self.enforce_project_guard(&workspace),
                Err(err) => Some(crate::ai_error(
                    "STORE_ERROR",
                    &crate::format_store_error(err),
                )),
            },
            Err(err) => Some(crate::ai_error(
                "STORE_ERROR",
                &crate::format_store_error(err),
            )),
        }
    }

    fn enforce_project_guard(&mut self, workspace: &crate::WorkspaceId) -> Option<Value> {
        let Some(expected) = self.project_guard.as_deref() else {
            return None;
        };
        match self
            .store
            .workspace_project_guard_ensure(workspace, expected)
        {
            Ok(()) => None,
            Err(crate::StoreError::ProjectGuardMismatch { expected, stored }) => {
                Some(crate::ai_error_with(
                    "PROJECT_GUARD_MISMATCH",
                    "Workspace belongs to a different project guard",
                    Some(&format!(
                        "Expected project_guard={expected}, but workspace is guarded as {stored}.",
                    )),
                    Vec::new(),
                ))
            }
            Err(crate::StoreError::InvalidInput(msg)) => {
                Some(crate::ai_error("INVALID_INPUT", msg))
            }
            Err(err) => Some(crate::ai_error(
                "STORE_ERROR",
                &crate::format_store_error(err),
            )),
        }
    }

    pub(crate) fn tool_storage(&mut self, _args: Value) -> Value {
        crate::ai_ok(
            "storage",
            json!( {
                "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
            }),
        )
    }
}

fn is_portal_tool(name: &str) -> bool {
    matches!(
        name,
        "status"
            | "macro_branch_note"
            | "tasks_macro_start"
            | "tasks_macro_close_step"
            | "tasks_snapshot"
    )
}

fn apply_portal_default_budgets(
    toolset: crate::Toolset,
    name: &str,
    args_obj: &mut serde_json::Map<String, Value>,
) {
    // Line protocol outputs always replace the JSON payload, so max_chars defaults are irrelevant
    // and can introduce noisy BUDGET_* warnings. Keep budgets opt-in for fmt=lines.
    let fmt = args_obj.get("fmt").and_then(|v| v.as_str());
    if crate::is_lines_fmt(fmt) {
        return;
    }

    let default_status_max_chars = match toolset {
        crate::Toolset::Core => 2000,
        crate::Toolset::Daily => 2500,
        crate::Toolset::Full => return,
    };
    let default_snapshot_max_chars = match toolset {
        crate::Toolset::Core => 6000,
        crate::Toolset::Daily => 9000,
        crate::Toolset::Full => return,
    };
    let default_resume_max_chars = match toolset {
        crate::Toolset::Core => 6000,
        crate::Toolset::Daily => 9000,
        crate::Toolset::Full => return,
    };

    match name {
        "status" => {
            if !args_obj.contains_key("max_chars") {
                args_obj.insert(
                    "max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_status_max_chars as u64)),
                );
            }
        }
        "tasks_snapshot" => {
            if !args_obj.contains_key("max_chars") {
                args_obj.insert(
                    "max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_snapshot_max_chars as u64)),
                );
            }
        }
        "tasks_macro_start" | "tasks_macro_close_step" => {
            if !args_obj.contains_key("resume_max_chars") {
                args_obj.insert(
                    "resume_max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_resume_max_chars as u64)),
                );
            }
        }
        _ => {}
    }
}

fn apply_read_tool_default_budgets(name: &str, args_obj: &mut serde_json::Map<String, Value>) {
    // Keep budgets opt-in for BM-L1 line outputs (they render warnings as extra lines).
    let fmt = args_obj.get("fmt").and_then(|v| v.as_str());
    if crate::is_lines_fmt(fmt) {
        return;
    }
    if args_obj.contains_key("max_chars") || args_obj.contains_key("context_budget") {
        return;
    }

    if !read_tool_accepts_budget(name) {
        return;
    }

    // Default budgets are intentionally generous but bounded. The goal is to remove
    // "limit juggling" for the common case, while still keeping the output deterministic.
    let default_context_budget = match name {
        // "Pack" tools are likely to be pasted directly into an agent context window.
        "tasks_resume_super" | "context_pack" | "think_pack" | "think_watch" => 20_000usize,

        // Read views that can become large quickly in active projects.
        "tasks_context"
        | "tasks_resume_pack"
        | "tasks_context_pack"
        | "tasks_radar"
        | "tasks_handoff"
        | "think_context"
        | "think_frontier"
        | "think_query"
        | "think_next"
        | "show"
        | "diff"
        | "log"
        | "docs_list"
        | "tag_list"
        | "reflog"
        | "branch_list"
        | "graph_query"
        | "graph_validate"
        | "graph_diff"
        | "graph_conflicts"
        | "graph_conflict_show"
        | "trace_hydrate"
        | "trace_validate"
        | "transcripts_open"
        | "transcripts_digest"
        | "transcripts_search"
        | "help"
        | "diagnostics" => 16_000usize,

        // Safe default for other read-ish tools that accept max_chars.
        _ => 12_000usize,
    };

    // Prefer context_budget when available (it behaves as a max_chars alias and can
    // deterministically shift default views toward smart retrieval).
    if read_tool_supports_context_budget(name) {
        args_obj.insert(
            "context_budget".to_string(),
            Value::Number(serde_json::Number::from(default_context_budget as u64)),
        );
    } else {
        args_obj.insert(
            "max_chars".to_string(),
            Value::Number(serde_json::Number::from(default_context_budget as u64)),
        );
    }
}

fn inject_smart_navigation_suggestions(
    tool: &str,
    args: &Value,
    resp_obj: &mut serde_json::Map<String, Value>,
) {
    if is_portal_tool(tool) {
        return;
    }
    if resp_obj
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        != true
    {
        return;
    }

    let budget_warning = auto_budget_escalation_allowlist(tool)
        && response_obj_has_budget_truncation_warning(resp_obj);
    let budget_snapshot = if budget_warning {
        extract_budget_snapshot_from_obj(resp_obj)
    } else {
        None
    };
    let next_cursor = extract_result_next_cursor(resp_obj);

    let Some(suggestions) = resp_obj
        .get_mut("suggestions")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    if !suggestions.is_empty() {
        return;
    }

    // 1) Budget friction: if the response was truncated, give a single "show more" action
    // that replays the same call with a larger budget. This keeps agents out of manual
    // max_chars guessing while preserving determinism (suggestion only; no auto-writes).
    if let Some((current_max_chars, used_chars)) = budget_snapshot
        && let Some(args_obj) = args.as_object()
    {
        let cap = auto_budget_escalation_cap_chars(tool);
        if current_max_chars < cap {
            let used = used_chars.unwrap_or(current_max_chars);
            let mut next_max_chars = current_max_chars
                .saturating_mul(2)
                .max(used.saturating_mul(2))
                .max(current_max_chars.saturating_add(1));
            if next_max_chars > cap {
                next_max_chars = cap;
            }

            if next_max_chars > current_max_chars {
                let mut params = args_obj.clone();
                apply_auto_escalated_budget(&mut params, next_max_chars);
                suggestions.push(crate::suggest_call(
                    tool,
                    "Show more (increase output budget).",
                    "high",
                    Value::Object(params),
                ));
                return;
            }
        }
    }

    // 2) "Button-like" navigation: if a result has a next_cursor, offer a single "show more"
    // pagination action (no extra parameters beyond cursor).
    if let Some(next_cursor) = next_cursor
        && let Some(args_obj) = args.as_object()
    {
        let mut params = args_obj.clone();
        params.insert(
            "cursor".to_string(),
            Value::Number(serde_json::Number::from(next_cursor)),
        );
        suggestions.push(crate::suggest_call(
            tool,
            "Show more (next page).",
            "medium",
            Value::Object(params),
        ));
    }
}

fn response_obj_has_budget_truncation_warning(resp_obj: &serde_json::Map<String, Value>) -> bool {
    let Some(warnings) = resp_obj.get("warnings").and_then(|v| v.as_array()) else {
        return false;
    };
    warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    })
}

fn extract_budget_snapshot_from_obj(
    resp_obj: &serde_json::Map<String, Value>,
) -> Option<(usize, Option<usize>)> {
    let budget = resp_obj.get("result")?.get("budget")?;
    let max_chars = budget.get("max_chars")?.as_u64()? as usize;
    let used_chars = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    Some((max_chars, used_chars))
}

fn extract_result_next_cursor(resp_obj: &serde_json::Map<String, Value>) -> Option<i64> {
    let pagination = resp_obj.get("result")?.get("pagination")?;
    let has_more = pagination
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !has_more {
        return None;
    }
    pagination.get("next_cursor")?.as_i64()
}

fn read_tool_accepts_budget(name: &str) -> bool {
    matches!(
        name,
        // Tasks reads
        "tasks_context"
            | "tasks_delta"
            | "tasks_radar"
            | "tasks_handoff"
            | "tasks_context_pack"
            | "tasks_resume_pack"
            | "tasks_resume_super"
            // Core reasoning reads / packs
            | "help"
            | "diagnostics"
            | "context_pack"
            // Reasoning packs & reads
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "show"
            | "diff"
            | "log"
            | "docs_list"
            | "tag_list"
            | "reflog"
            | "graph_query"
            | "graph_validate"
            | "graph_diff"
            | "graph_conflicts"
            | "graph_conflict_show"
            | "branch_list"
            | "trace_hydrate"
            | "trace_validate"
            | "transcripts_open"
            | "transcripts_digest"
            | "transcripts_search"
    )
}

fn read_tool_supports_context_budget(name: &str) -> bool {
    matches!(
        name,
        "tasks_resume_super"
            | "tasks_snapshot"
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "context_pack"
    )
}

fn response_has_budget_truncation_warning(resp: &Value) -> bool {
    let Some(warnings) = resp.get("warnings").and_then(|v| v.as_array()) else {
        return false;
    };
    warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    })
}

fn extract_budget_snapshot(resp: &Value) -> Option<(usize, Option<usize>)> {
    let budget = resp.get("result")?.get("budget")?;
    let max_chars = budget.get("max_chars")?.as_u64()? as usize;
    let used_chars = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    Some((max_chars, used_chars))
}

fn auto_budget_escalation_allowlist(name: &str) -> bool {
    matches!(
        name,
        "tasks_context"
            | "tasks_resume_pack"
            | "tasks_resume_super"
            | "tasks_context_pack"
            | "tasks_delta"
            | "tasks_radar"
            | "tasks_handoff"
            | "help"
            | "diagnostics"
            | "context_pack"
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "show"
            | "diff"
            | "log"
            | "docs_list"
            | "tag_list"
            | "reflog"
            | "graph_query"
            | "graph_validate"
            | "graph_diff"
            | "graph_conflicts"
            | "graph_conflict_show"
            | "branch_list"
            | "trace_hydrate"
            | "trace_validate"
            | "transcripts_open"
            | "transcripts_digest"
            | "transcripts_search"
    )
}

fn auto_budget_escalation_cap_chars(_name: &str) -> usize {
    // Hard cap to prevent runaway responses even under repeated retries.
    200_000
}

fn apply_auto_escalated_budget(args_obj: &mut serde_json::Map<String, Value>, max_chars: usize) {
    let next = Value::Number(serde_json::Number::from(max_chars as u64));
    let mut applied = false;
    if args_obj.contains_key("context_budget") {
        args_obj.insert("context_budget".to_string(), next.clone());
        applied = true;
    }
    if args_obj.contains_key("max_chars") {
        args_obj.insert("max_chars".to_string(), next.clone());
        applied = true;
    }
    if !applied {
        args_obj.insert("max_chars".to_string(), next);
    }
}

fn advertised_tool_names(toolset: crate::Toolset) -> HashSet<String> {
    crate::tools::tool_definitions(toolset)
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<HashSet<_>>()
}

fn escalation_toolset_for_hidden(
    hidden_targets: &[String],
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) -> Option<&'static str> {
    let mut needs_daily = false;
    let mut needs_full = false;
    for target in hidden_targets {
        if core_tools.contains(target) {
            continue;
        }
        if daily_tools.contains(target) {
            needs_daily = true;
        } else {
            needs_full = true;
        }
    }

    if needs_full {
        Some("full")
    } else if needs_daily {
        Some("daily")
    } else {
        None
    }
}

fn portal_recovery_suggestion(
    target: &str,
    params: &Value,
    _tool: &str,
    args: &Value,
    error_code: Option<&str>,
    default_workspace: Option<&str>,
) -> Option<Value> {
    match (target, error_code) {
        ("init", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "status",
                "Auto-init workspace and show status (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_templates_list", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_templates_list",
                "List built-in templates.",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_verify", Some("CHECKPOINTS_NOT_CONFIRMED")) => {
            let mut call_params = serde_json::Map::new();

            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some(task) = params.get("task").and_then(|v| v.as_str()) {
                call_params.insert("task".to_string(), Value::String(task.to_string()));
            }
            if let Some(step_id) = params.get("step_id").and_then(|v| v.as_str()) {
                call_params.insert("step_id".to_string(), Value::String(step_id.to_string()));
            }
            if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
                call_params.insert("path".to_string(), Value::String(path.to_string()));
            }
            let checkpoints = params
                .get("checkpoints")
                .cloned()
                .unwrap_or(Value::String("gate".to_string()));
            call_params.insert("checkpoints".to_string(), checkpoints);

            maybe_omit_default_workspace(&mut call_params, default_workspace);

            Some(crate::suggest_call(
                "tasks_macro_close_step",
                "Confirm missing checkpoints + close step (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_context", Some("REVISION_MISMATCH")) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                call_params.insert(key.to_string(), Value::String(id));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_snapshot",
                "Refresh snapshot to get the current revision and retry (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_resume", _) | ("tasks_resume_pack", _) | ("tasks_resume_super", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some(task) = params.get("task").and_then(|v| v.as_str()) {
                call_params.insert("task".to_string(), Value::String(task.to_string()));
            }
            if let Some(plan) = params.get("plan").and_then(|v| v.as_str()) {
                call_params.insert("plan".to_string(), Value::String(plan.to_string()));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_snapshot",
                "Use snapshot (portal) instead of low-level resume views.",
                "medium",
                Value::Object(call_params),
            ))
        }
        _ => None,
    }
}

fn inject_portal_recovery_for_error(
    tool: &str,
    args: &Value,
    error_code: Option<&str>,
    error_message: Option<&str>,
    suggestions: &mut Vec<Value>,
    default_workspace: Option<&str>,
) {
    // Recovery UX applies to the whole server surface, but keep it conservative:
    // - Only run when there are no suggestions at all.
    // - Only inject for the tasks subsystem (daily DX driver), to avoid surprising
    //   behavior in unrelated tool families.
    if !tool.starts_with("tasks_") {
        return;
    }
    if !suggestions.is_empty() {
        return;
    }

    let workspace = args
        .as_object()
        .and_then(|obj| obj.get("workspace"))
        .and_then(|v| v.as_str());

    match error_code {
        Some("UNKNOWN_ID") => {
            // Keep the agent productive without forcing a full toolset disclosure.
            // - If a step selector was wrong, show a snapshot for the current target (if any).
            // - If a target id was wrong, show snapshot for focus (drop explicit target), plus a
            //   safe portal fallback to re-establish focus.
            // - If focus itself is broken, suggest starting a new task (portal).
            let msg = error_message.unwrap_or("");
            let is_step_like = msg.contains("Step not found")
                || msg.contains("Parent step not found")
                || msg.contains("Task node not found");

            if is_step_like {
                let mut call_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                    call_params.insert(key.to_string(), Value::String(id));
                }
                maybe_omit_default_workspace(&mut call_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_snapshot",
                    "Open snapshot to confirm ids and selectors (portal).",
                    "high",
                    Value::Object(call_params),
                ));
                return;
            }

            let has_explicit_target = extract_task_or_plan_from_args(args).is_some();
            if has_explicit_target {
                let mut call_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                // Intentionally omit task/plan: a stale id should not keep failing. Prefer focus.
                maybe_omit_default_workspace(&mut call_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_snapshot",
                    "Open snapshot (portal) to confirm focus and valid ids.",
                    "high",
                    Value::Object(call_params),
                ));

                let mut start_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    start_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                start_params.insert(
                    "task_title".to_string(),
                    Value::String("New task".to_string()),
                );
                maybe_omit_default_workspace(&mut start_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_macro_start",
                    "If focus is missing, restore it by starting a new task (portal).",
                    "medium",
                    Value::Object(start_params),
                ));
                return;
            }

            let mut start_params = serde_json::Map::new();
            if let Some(ws) = workspace {
                start_params.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            start_params.insert(
                "task_title".to_string(),
                Value::String("New task".to_string()),
            );
            maybe_omit_default_workspace(&mut start_params, default_workspace);
            suggestions.push(crate::suggest_call(
                "tasks_macro_start",
                "Restore focus by starting a new task (portal).",
                "high",
                Value::Object(start_params),
            ));
        }
        Some("REVISION_MISMATCH") => {
            // Fail-safe: if an implementation forgets to include a refresh hint, provide one.
            let mut call_params = serde_json::Map::new();
            if let Some(ws) = workspace {
                call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                call_params.insert(key.to_string(), Value::String(id));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            suggestions.push(crate::suggest_call(
                "tasks_snapshot",
                "Refresh snapshot to get the current revision (portal).",
                "high",
                Value::Object(call_params),
            ));
        }
        _ => {}
    }
}

fn maybe_omit_default_workspace(
    params: &mut serde_json::Map<String, Value>,
    default_workspace: Option<&str>,
) {
    let Some(default_workspace) = default_workspace else {
        return;
    };
    if params
        .get("workspace")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v == default_workspace)
    {
        params.remove("workspace");
    }
}

fn extract_task_or_plan_from_args(args: &Value) -> Option<(&'static str, String)> {
    let obj = args.as_object()?;
    if let Some(task) = obj.get("task").and_then(|v| v.as_str()) {
        return Some(("task", task.to_string()));
    }
    if let Some(plan) = obj.get("plan").and_then(|v| v.as_str()) {
        return Some(("plan", plan.to_string()));
    }
    None
}
