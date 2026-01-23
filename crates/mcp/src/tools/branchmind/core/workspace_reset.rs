#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_workspace_reset(&mut self, args: Value) -> Value {
        if !args.is_object() {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        }

        let previous = self
            .workspace_override
            .as_deref()
            .or(self.default_workspace.as_deref())
            .map(|v| v.to_string());
        self.workspace_override = None;

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
            "workspace_reset",
            json!({
                "workspace": effective,
                "previous": previous,
                "default_workspace": self.default_workspace.clone(),
                "workspace_override": self.workspace_override.clone(),
                "workspace_mode": mode
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
