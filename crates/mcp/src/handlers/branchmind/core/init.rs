#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_init(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        match self.store.workspace_init(&workspace) {
            Ok(()) => {
                let checkout = self.store.branch_checkout_get(&workspace).ok().flatten();
                let defaults = json!({
                    "branch": self.store.default_branch_name(),
                    "docs": {
                        "notes": DEFAULT_NOTES_DOC,
                        "graph": DEFAULT_GRAPH_DOC,
                        "trace": DEFAULT_TRACE_DOC
                    }
                });
                let mut suggestions = Vec::new();
                if checkout.is_some() {
                    suggestions.push(suggest_call(
                        "think_card",
                        "Start with a lightweight note.",
                        "high",
                        json!({ "workspace": workspace.as_str(), "card": "First note" }),
                    ));
                }
                ai_ok_with(
                    "init",
                    json!({
                        "workspace": workspace.as_str(),
                        "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
                        "schema_version": "v0",
                        "checkout": checkout,
                        "defaults": defaults
                    }),
                    suggestions,
                )
            }
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
