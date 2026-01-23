#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_workspace_use(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let requested = workspace.as_str().to_string();

        if self.workspace_lock
            && let Some(default_workspace) = self.default_workspace.as_deref()
            && requested != default_workspace
        {
            return ai_error_with(
                "WORKSPACE_LOCKED",
                "workspace is locked to the configured default workspace",
                Some("Restart the server without workspace lock to switch workspaces in-session."),
                vec![suggest_call(
                    "status",
                    "Inspect the current workspace policy.",
                    "low",
                    json!({}),
                )],
            );
        }

        if let Some(allowlist) = self.workspace_allowlist.as_ref()
            && !allowlist.iter().any(|allowed| allowed == &requested)
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
            return ai_error_with(
                "WORKSPACE_NOT_ALLOWED",
                "workspace is not in the allowlist",
                Some(&hint),
                Vec::new(),
            );
        }

        let previous = self
            .workspace_override
            .as_deref()
            .or(self.default_workspace.as_deref())
            .map(|v| v.to_string());

        let cleared = self
            .default_workspace
            .as_deref()
            .is_some_and(|default| default == requested);
        if cleared {
            self.workspace_override = None;
        } else {
            self.workspace_override = Some(requested.clone());
        }

        let effective = self
            .workspace_override
            .as_deref()
            .or(self.default_workspace.as_deref())
            .map(|v| v.to_string());

        let mode = if self.workspace_allowlist.is_some() {
            "allowlist"
        } else if self.workspace_explicit {
            "explicit"
        } else {
            "auto"
        };

        ai_ok_with(
            "workspace_use",
            json!({
                "workspace": effective,
                "previous": previous,
                "default_workspace": self.default_workspace.clone(),
                "workspace_override": self.workspace_override.clone(),
                "workspace_mode": mode,
                "override_cleared": cleared
            }),
            vec![suggest_call(
                "status",
                "Confirm the active workspace and policy.",
                "high",
                json!({}),
            )],
        )
    }
}
