#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_storage(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        ai_ok(
            "tasks_storage",
            json!({
                "workspace": workspace.as_str(),
                "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
                "defaults": {
                    "branch": self.store.default_branch_name(),
                    "docs": {
                        "notes": DEFAULT_NOTES_DOC,
                        "graph": DEFAULT_GRAPH_DOC,
                        "trace": DEFAULT_TRACE_DOC
                    }
                }
            }),
        )
    }
}
