#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;
use std::collections::HashSet;

impl McpServer {
    pub(super) fn preprocess_args(&mut self, name: &str, args: &mut Value) -> Option<Value> {
        let args_obj = args.as_object_mut()?;

        // DX: when a default workspace is configured, treat it as the implicit workspace
        // for all tool calls unless the caller explicitly provides `workspace`.
        //
        // This keeps daily usage cheap (no boilerplate) and makes BM-L1 "copy/paste" commands
        // usable across restarts when the server is scoped to a single project.
        if let Some(default_workspace) = self.default_workspace.as_deref()
            && !args_obj.contains_key("workspace")
        {
            args_obj.insert(
                "workspace".to_string(),
                Value::String(default_workspace.to_string()),
            );
        }

        // Multi-agent / concurrency DX:
        // When a default agent_id is configured, apply it to **tasks_* only** (step leases).
        //
        // Meaning-mode memory is shared-by-default and must not depend on an injected agent id.
        // Callers can still pass agent_id explicitly for audit or explicit multi-agent semantics.
        let tool_accepts_default_agent_id = name.starts_with("tasks_");
        if tool_accepts_default_agent_id
            && let Some(default_agent_id) = self.default_agent_id.as_deref()
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
                    serde_json::json!({}),
                )],
            ));
        }

        // AI-first invariant: portal tools are always context-first (BM-L1 lines).
        // Do not expose / depend on a json-vs-lines toggle in portals.
        if super::portal::is_portal_tool(name) {
            args_obj.insert("fmt".to_string(), Value::String("lines".to_string()));
        }

        // Daily DX: treat jobs_radar as an inbox (BM-L1 lines) in reduced toolsets.
        //
        // NOTE: jobs_radar must *not* be a portal tool (portals always force fmt=lines) because
        // some clients/automation may rely on JSON outputs in the full toolset.
        if self.toolset != crate::Toolset::Full
            && name == "tasks_jobs_radar"
            && !args_obj.contains_key("fmt")
        {
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

        // Portal UX: keep portal calls cheap by defaulting budgets when the caller didn't specify
        // them explicitly. Even in fmt=lines mode the tool still builds a structured payload and
        // may emit BUDGET_* warnings — defaults + auto-escalation remove “limit juggling”.
        if super::portal::is_portal_tool(name) {
            super::budgets::apply_portal_default_budgets(self.toolset, name, args_obj);
        }

        // Full toolset DX: for heavy read tools, apply deterministic default budgets when the
        // caller didn't opt into explicit max_chars/context_budget. This prevents accidental
        // context blowups while keeping callers fully in control once they specify budgets.
        if !super::portal::is_portal_tool(name) {
            super::budgets::apply_read_tool_default_budgets(name, args_obj);
        }
        if self.default_workspace.is_none()
            && super::portal::is_portal_tool(name)
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

    pub(super) fn postprocess_response(&self, tool: &str, args: &Value, response: &mut Value) {
        let fmt = args.get("fmt").and_then(|v| v.as_str());
        let wants_lines = crate::is_lines_fmt(fmt);

        let Some(resp_obj) = response.as_object_mut() else {
            return;
        };

        if !wants_lines && self.toolset == crate::Toolset::Full {
            super::suggestions::inject_smart_navigation_suggestions(tool, args, resp_obj);
            return;
        }

        if self.toolset != crate::Toolset::Full {
            let advertised = super::suggestions::advertised_tool_names(self.toolset);
            let core_tools = super::suggestions::advertised_tool_names(crate::Toolset::Core);
            let daily_tools = super::suggestions::advertised_tool_names(crate::Toolset::Daily);

            if let Some(result) = resp_obj.get_mut("result") {
                super::suggestions::sanitize_engine_calls_in_value(
                    result,
                    &advertised,
                    &core_tools,
                    &daily_tools,
                );
            }

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
                super::suggestions::inject_portal_recovery_for_error(
                    tool,
                    args,
                    error_code.as_deref(),
                    error_message.as_deref(),
                    suggestions,
                    self.default_workspace.as_deref(),
                );
            }
            if !suggestions.is_empty() {
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
                            if let Some(portal) = super::suggestions::portal_recovery_suggestion(
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

                if let Some(escalation_toolset) = super::suggestions::escalation_toolset_for_hidden(
                    &hidden_targets,
                    &core_tools,
                    &daily_tools,
                ) {
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
                                serde_json::json!({ "toolset": escalation_toolset }),
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

    pub(super) fn auto_init_workspace(
        &mut self,
        args: &serde_json::Map<String, Value>,
    ) -> Option<Value> {
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

    pub(super) fn enforce_project_guard(
        &mut self,
        workspace: &crate::WorkspaceId,
    ) -> Option<Value> {
        let expected = self.project_guard.as_deref()?;
        match self
            .store
            .workspace_project_guard_ensure(workspace, expected)
        {
            Ok(()) => None,
            Err(crate::StoreError::ProjectGuardMismatch { expected, stored }) => {
                if self.project_guard_rebind_enabled {
                    if let Err(err) = self
                        .store
                        .workspace_project_guard_rebind(workspace, &expected)
                    {
                        return Some(crate::ai_error(
                            "STORE_ERROR",
                            &crate::format_store_error(err),
                        ));
                    }
                    return None;
                }
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
}
