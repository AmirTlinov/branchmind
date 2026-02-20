#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;
use std::path::{Path, PathBuf};

impl McpServer {
    pub(super) fn preprocess_args(&mut self, name: &str, args: &mut Value) -> Option<Value> {
        let args_obj = args.as_object_mut()?;

        // DX: allow configuring the default workspace via filesystem path (CLI/env).
        // We normalize once per session on first use so workspace_lock/allowlist comparisons
        // operate on the final WorkspaceId (not the raw path string).
        if let Some(raw) = self.default_workspace.clone()
            && crate::WorkspaceId::try_new(raw.clone()).is_err()
            && looks_like_workspace_path(&raw)
        {
            match self.workspace_id_from_path(&raw) {
                Ok(resolved) => self.default_workspace = Some(resolved),
                Err(resp) => return Some(resp),
            }
        }

        let effective_default = self
            .workspace_override
            .as_deref()
            .or(self.default_workspace.as_deref());
        let skip_workspace_injection = matches!(name, "workspace_use" | "workspace_reset");

        // v1 portals: lift `workspace` / `fmt` out of the nested envelope so the
        // legacy pipeline (workspace guards, line protocol formatting) continues to work.
        //
        // This must happen **before** default workspace injection; otherwise we could override
        // an explicit inner workspace with the session default.
        if crate::tools_v1::is_v1_tool(name) {
            let inner_ws = args_obj
                .get("args")
                .and_then(|v| v.get("workspace"))
                .cloned();
            let inner_fmt = args_obj.get("args").and_then(|v| v.get("fmt")).cloned();

            if !args_obj.contains_key("workspace")
                && let Some(ws) = inner_ws
            {
                args_obj.insert("workspace".to_string(), ws);
            }
            if !args_obj.contains_key("fmt")
                && let Some(fmt) = inner_fmt
            {
                args_obj.insert("fmt".to_string(), fmt);
            }

            // DX shorthand: accept `system(op=schema.get, cmd=\"tasks.snapshot\")` by lifting the
            // top-level cmd into args.cmd when args.cmd is missing.
            if name == "system" && args_obj.get("op").and_then(|v| v.as_str()) == Some("schema.get")
            {
                let top_level_cmd = args_obj
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(cmd) = top_level_cmd {
                    let args_entry = args_obj
                        .entry("args".to_string())
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    if let Some(args_inner) = args_entry.as_object_mut()
                        && !args_inner.contains_key("cmd")
                    {
                        args_inner.insert("cmd".to_string(), Value::String(cmd));
                    }
                }
            }
        }

        // v1 DX: in the daily toolset, portal calls default to fmt=lines even when callers
        // omit fmt explicitly. This keeps the “state + command” path cheap-by-default.
        //
        // Important: only enable this default for cmds that have a stable BM-L1 renderer.
        // Long-tail ops should continue returning structured JSON unless the caller opts in.
        if self.toolset == crate::Toolset::Daily && !args_obj.contains_key("fmt") {
            let cmd = args_obj.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            let wants_default_lines = match name {
                "status" => true,
                "workspace" => matches!(cmd, "workspace.use" | "workspace.reset"),
                "tasks" => matches!(
                    cmd,
                    "tasks.macro.start"
                        | "tasks.macro.delegate"
                        | "tasks.macro.close.step"
                        | "tasks.snapshot"
                ),
                "jobs" => matches!(
                    cmd,
                    "jobs.list" | "jobs.radar" | "jobs.open" | "jobs.tail" | "jobs.message"
                ),
                _ => false,
            };
            if wants_default_lines {
                args_obj.insert("fmt".to_string(), Value::String("lines".to_string()));
            }
        }

        // DX: when a default workspace is configured, treat it as the implicit workspace
        // for all tool calls unless the caller explicitly provides `workspace`.
        //
        // This keeps daily usage cheap (no boilerplate) and makes BM-L1 "copy/paste" commands
        // usable across restarts when the server is scoped to a single project.
        if !skip_workspace_injection
            && let Some(default_workspace) = effective_default
            && !args_obj.contains_key("workspace")
        {
            args_obj.insert(
                "workspace".to_string(),
                Value::String(default_workspace.to_string()),
            );
        }

        // DX: accept workspace as a filesystem path (e.g. "/home/me/repo") and resolve it to a
        // stable WorkspaceId *before* allowlist/lock checks run.
        if let Some(raw) = args_obj.get("workspace").and_then(|v| v.as_str()) {
            let raw = raw.trim().to_string();
            if !raw.is_empty()
                && crate::WorkspaceId::try_new(raw.clone()).is_err()
                && looks_like_workspace_path(&raw)
            {
                match self.workspace_id_from_path(&raw) {
                    Ok(resolved) => {
                        args_obj.insert("workspace".to_string(), Value::String(resolved));
                    }
                    Err(resp) => return Some(resp),
                }
            }
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
            let mut retry_args = args_obj.clone();
            retry_args.remove("workspace");
            let mut suggestions = Vec::new();
            if retry_args.is_empty() {
                suggestions.push(crate::suggest_call(
                    name,
                    "Retry using the default workspace (omit workspace).",
                    "high",
                    serde_json::json!({}),
                ));
            } else {
                suggestions.push(crate::suggest_call(
                    name,
                    "Retry using the default workspace (omit workspace).",
                    "high",
                    Value::Object(retry_args),
                ));
            }
            return Some(crate::ai_error_with(
                "WORKSPACE_LOCKED",
                "workspace is locked to the configured default workspace",
                Some(
                    "Drop the workspace argument (use the default) or restart the server without workspace lock.",
                ),
                suggestions,
            ));
        }

        if let Some(allowlist) = self.workspace_allowlist.as_ref()
            && let Some(workspace) = args_obj.get("workspace").and_then(|v| v.as_str())
            && !allowlist.iter().any(|allowed| allowed == workspace)
        {
            let mut allowed = allowlist.clone();
            allowed.sort();
            let limit = allowed.len().min(5);
            let preview = allowed
                .iter()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let hint = if allowed.len() > limit {
                format!(
                    "Allowed workspaces (showing {limit} of {}): {preview}",
                    allowed.len()
                )
            } else {
                format!("Allowed workspaces: {preview}")
            };
            let preferred = self
                .default_workspace
                .as_deref()
                .and_then(|ws| {
                    if allowlist.iter().any(|allowed| allowed == ws) {
                        Some(ws.to_string())
                    } else {
                        None
                    }
                })
                .or_else(|| allowed.first().cloned());
            let mut suggestions = Vec::new();
            if let Some(preferred) = preferred {
                let mut retry_args = args_obj.clone();
                retry_args.insert("workspace".to_string(), Value::String(preferred.clone()));
                suggestions.push(crate::suggest_call(
                    name,
                    "Retry with an allowed workspace.",
                    "high",
                    Value::Object(retry_args),
                ));
                if name != "workspace_use" {
                    suggestions.push(crate::suggest_call(
                        "workspace_use",
                        "Switch the session workspace.",
                        "medium",
                        serde_json::json!({ "workspace": preferred }),
                    ));
                }
            }
            return Some(crate::ai_error_with(
                "WORKSPACE_NOT_ALLOWED",
                "workspace is not in the allowlist",
                Some(&hint),
                suggestions,
            ));
        }

        // Legacy portal tools (core/daily): prefer BM-L1 line outputs in reduced toolsets.
        //
        // In the full toolset, v1 tools should default to structured JSON envelopes so
        // contract tests can assert on actions/refs deterministically.
        if self.toolset != crate::Toolset::Full && super::portal::is_portal_tool(name) {
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

        // v1 portals: in reduced toolsets we keep status as BM-L1 tagged lines by
        // default (state + one safe next command). In full toolset, status defaults to the
        // structured v1 envelope (actions-first).
        if self.toolset != crate::Toolset::Full && name == "status" && !args_obj.contains_key("fmt")
        {
            args_obj.insert("fmt".to_string(), Value::String("lines".to_string()));
        }

        // v1 portals: preserve the low-noise portal defaults for the canonical macros.
        //
        // We do *not* force fmt=lines for all tasks calls — only for the portal-grade macros
        // that are designed to be read as BM-L1 handoff lines.
        if name == "tasks"
            && !args_obj.contains_key("fmt")
            && let Some(cmd) = args_obj.get("cmd").and_then(|v| v.as_str())
        {
            let wants_lines = matches!(
                cmd,
                "tasks.macro.start" | "tasks.macro.delegate" | "tasks.macro.close.step"
            ) || (self.toolset != crate::Toolset::Full
                && cmd == "tasks.snapshot");
            if wants_lines {
                args_obj.insert("fmt".to_string(), Value::String("lines".to_string()));
            }
        }

        // Daily DX: treat jobs.radar as an inbox (BM-L1 lines) in reduced toolsets.
        if self.toolset != crate::Toolset::Full
            && name == "jobs"
            && !args_obj.contains_key("fmt")
            && args_obj
                .get("cmd")
                .and_then(|v| v.as_str())
                .is_some_and(|cmd| cmd == "jobs.radar")
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
            super::budgets::apply_portal_default_budgets(
                self.toolset,
                self.dx_mode,
                name,
                args_obj,
            );
        }

        // Full toolset DX: for heavy read tools, apply deterministic default budgets when the
        // caller didn't opt into explicit max_chars/context_budget. This prevents accidental
        // context blowups while keeping callers fully in control once they specify budgets.
        if !super::portal::is_portal_tool(name) {
            super::budgets::apply_read_tool_default_budgets(name, args_obj);
        }
        if self.default_workspace.is_none()
            && self.workspace_override.is_none()
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

        if !skip_workspace_injection && let Some(resp) = self.auto_init_workspace(args_obj) {
            return Some(resp);
        }
        // v1 portals: normalize nested args first so legacy target aliases continue to
        // work (e.g. `target={id,kind}` → `task=` / `plan=` for tasks).
        if crate::tools_v1::is_v1_tool(name)
            && let Some(inner) = args_obj.get_mut("args").and_then(|v| v.as_object_mut())
            && let Err(resp) = crate::normalize_target_map(name, inner)
        {
            return Some(resp);
        }
        if let Err(resp) = crate::normalize_target_map(name, args_obj) {
            return Some(resp);
        }
        None
    }

    pub(super) fn postprocess_response(&self, tool: &str, args: &Value, response: &mut Value) {
        let fmt = args.get("fmt").and_then(|v| v.as_str()).or_else(|| {
            args.get("args")
                .and_then(|v| v.get("fmt"))
                .and_then(|v| v.as_str())
        });
        let wants_lines = crate::is_lines_fmt(fmt);

        let Some(resp_obj) = response.as_object_mut() else {
            return;
        };

        // v1: suggestions[] are reserved (always empty). We intentionally do not emit
        // suggestions as a parallel "next steps" rail. All recoveries go through actions[].

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
            if let Some(actions) = resp_obj.get_mut("actions").and_then(|v| v.as_array_mut()) {
                super::actions::rewrite_actions_for_toolset(
                    self.toolset,
                    error_code.as_deref(),
                    actions,
                    self.default_workspace.as_deref(),
                );
            }

            // v1 invariant: suggestions[] are always empty.
            if let Some(suggestions) = resp_obj
                .get_mut("suggestions")
                .and_then(|v| v.as_array_mut())
            {
                suggestions.clear();
            }
        }

        if wants_lines {
            let (tool_for_lines, args_for_lines) = if crate::tools_v1::is_v1_tool(tool) {
                let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
                let mapped_tool = match tool {
                    "status" => Some("status"),
                    "workspace" => match cmd {
                        "workspace.use" => Some("workspace_use"),
                        "workspace.reset" => Some("workspace_reset"),
                        _ => None,
                    },
                    "tasks" => match cmd {
                        "tasks.macro.start" => Some("tasks_macro_start"),
                        "tasks.macro.delegate" => Some("tasks_macro_delegate"),
                        "tasks.macro.close.step" => Some("tasks_macro_close_step"),
                        "tasks.snapshot" => Some("tasks_snapshot"),
                        _ => None,
                    },
                    "jobs" => match cmd {
                        "jobs.list" => Some("tasks_jobs_list"),
                        "jobs.radar" => Some("tasks_jobs_radar"),
                        "jobs.open" => Some("tasks_jobs_open"),
                        "jobs.tail" => Some("tasks_jobs_tail"),
                        "jobs.message" => Some("tasks_jobs_message"),
                        _ => None,
                    },
                    _ => None,
                };

                let mut merged = args
                    .get("args")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                if let Some(ws) = args.get("workspace") {
                    merged
                        .entry("workspace".to_string())
                        .or_insert_with(|| ws.clone());
                }
                if let Some(fmt) = args.get("fmt") {
                    merged
                        .entry("fmt".to_string())
                        .or_insert_with(|| fmt.clone());
                }

                (mapped_tool.unwrap_or(tool), Value::Object(merged))
            } else {
                (tool, args.clone())
            };

            let omit_workspace = self.default_workspace.as_deref().is_some_and(|default_ws| {
                args_for_lines
                    .get("workspace")
                    .and_then(|v| v.as_str())
                    .is_some_and(|ws| ws == default_ws)
            });
            crate::apply_portal_line_format(
                tool_for_lines,
                &args_for_lines,
                response,
                self.toolset,
                omit_workspace,
            );
        }
    }

    pub(super) fn auto_init_workspace(
        &mut self,
        args: &mut serde_json::Map<String, Value>,
    ) -> Option<Value> {
        let workspace_raw = args.get("workspace").and_then(|v| v.as_str())?;
        let workspace_raw = workspace_raw.to_string();
        let workspace = match crate::WorkspaceId::try_new(workspace_raw.clone()) {
            Ok(v) => v,
            Err(_) => {
                // DX: avoid polluting follow-up actions with an invalid outer workspace field.
                args.remove("workspace");

                let suggested = Self::suggest_workspace_id(&workspace_raw);
                let recovery = format!(
                    "workspace must be a WorkspaceId (e.g. \"money1\"). You may also pass an absolute path (e.g. \"/home/me/repo\") and it will be mapped to an id. Fix: workspace=\"{suggested}\" (or call workspace op=use)."
                );
                return Some(crate::ai_error_with(
                    "INVALID_INPUT",
                    "workspace: expected WorkspaceId",
                    Some(recovery.as_str()),
                    vec![crate::suggest_call(
                        "workspace_use",
                        "Switch the session workspace.",
                        "high",
                        serde_json::json!({ "workspace": suggested }),
                    )],
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

    fn suggest_workspace_id(raw: &str) -> String {
        let raw = raw.trim();
        let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
        let mut out = String::new();
        let mut prev_dash = false;
        for ch in base.chars() {
            let lc = ch.to_ascii_lowercase();
            if lc.is_ascii_alphanumeric() {
                out.push(lc);
                prev_dash = false;
                continue;
            }
            if matches!(lc, '-' | '_' | '.' | ' ') {
                if !out.is_empty() && !prev_dash {
                    out.push('-');
                    prev_dash = true;
                }
                continue;
            }
            if !out.is_empty() && !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
        let trimmed = out.trim_matches('-');
        let mut suggested = if trimmed.is_empty() {
            "my-workspace".to_string()
        } else {
            trimmed.to_string()
        };
        if suggested.len() > 64 {
            suggested.truncate(64);
            suggested = suggested.trim_matches('-').to_string();
            if suggested.is_empty() {
                return "my-workspace".to_string();
            }
        }
        suggested
    }

    pub(crate) fn enforce_project_guard(
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

fn looks_like_workspace_path(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.is_empty() {
        return false;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return true;
    }
    if raw == "." || raw == ".." || raw.starts_with("./") || raw.starts_with("../") {
        return true;
    }
    if raw == "~" || raw.starts_with("~/") {
        return true;
    }
    if raw.contains('\\') {
        return true;
    }
    // Windows drive path: "C:\..." / "C:/..."
    if raw.len() >= 2 && raw.as_bytes().get(1) == Some(&b':') {
        return true;
    }
    false
}

impl McpServer {
    #[allow(dead_code)]
    pub(crate) fn workspace_id_resolve(&mut self, raw: &str) -> Result<String, crate::StoreError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(crate::StoreError::InvalidInput(
                "workspace must not be empty",
            ));
        }
        if crate::WorkspaceId::try_new(raw.to_string()).is_ok() {
            return Ok(raw.to_string());
        }
        if looks_like_workspace_path(raw) {
            return self.workspace_id_from_path_store(raw);
        }
        Err(crate::StoreError::InvalidInput(
            "workspace: expected WorkspaceId",
        ))
    }

    pub(crate) fn workspace_id_from_path(&mut self, raw: &str) -> Result<String, Value> {
        self.workspace_id_from_path_store(raw)
            .map_err(|err| match err {
                crate::StoreError::InvalidInput(msg) => crate::ai_error("INVALID_INPUT", msg),
                other => crate::ai_error("STORE_ERROR", &crate::format_store_error(other)),
            })
    }

    pub(crate) fn workspace_id_from_path_store(
        &mut self,
        raw: &str,
    ) -> Result<String, crate::StoreError> {
        let root = normalize_workspace_path(raw);
        let root_str = root.to_string_lossy().to_string();

        if let Some(existing) = self.store.workspace_path_resolve(&root_str)? {
            return Ok(existing.as_str().to_string());
        }

        let base = Self::suggest_workspace_id(&root_str);
        let mut workspace = crate::WorkspaceId::try_new(base.clone()).unwrap_or_else(|_| {
            crate::WorkspaceId::try_new("workspace".to_string()).expect("fallback workspace")
        });

        match self.store.workspace_exists(&workspace)? {
            false => {}
            true => match self.store.workspace_path_primary_get(&workspace)? {
                None => {}
                Some(bound) if bound == root_str => {}
                Some(_bound) => {
                    let hash = short_path_hash(&root_str);
                    let candidate = format!("{base}-{hash}");
                    workspace = crate::WorkspaceId::try_new(candidate)
                        .unwrap_or_else(|_| crate::WorkspaceId::try_new(base).expect("valid"));
                }
            },
        }

        self.store.workspace_path_bind(&workspace, &root_str)?;

        Ok(workspace.as_str().to_string())
    }
}

fn normalize_workspace_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    let expanded = if raw == "~" || raw.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            if raw == "~" {
                home
            } else {
                home.join(raw.trim_start_matches("~/"))
            }
        } else {
            PathBuf::from(raw)
        }
    } else {
        PathBuf::from(raw)
    };

    let mut absolute = if expanded.is_absolute() {
        expanded
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(expanded)
    } else {
        expanded
    };

    if let Ok(canon) = std::fs::canonicalize(&absolute) {
        absolute = canon;
    }

    if absolute.is_file()
        && let Some(parent) = absolute.parent()
    {
        absolute = parent.to_path_buf();
    }

    find_git_root(&absolute).unwrap_or(absolute)
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn short_path_hash(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    let full = format!("{hash:016x}");
    full.chars().take(8).collect::<String>()
}
