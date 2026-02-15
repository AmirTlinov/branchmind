#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_workspace_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 500),
            Err(resp) => return resp,
        };
        let offset = match optional_usize(args_obj, "offset") {
            Ok(v) => v.unwrap_or(0),
            Err(resp) => return resp,
        };

        // Detect truncation precisely by probing one extra item (bounded by store hard cap).
        let probe_limit = limit.saturating_add(1);
        let mut rows = match self.store.list_workspaces(probe_limit, offset) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let mut truncated_by_limit = false;
        if rows.len() > limit {
            truncated_by_limit = true;
            rows.truncate(limit);
        }

        let selected = self
            .workspace_override
            .clone()
            .or_else(|| self.default_workspace.clone());
        let selected_source = if self.workspace_override.is_some() {
            "workspace_override"
        } else if self.default_workspace.is_some() {
            "default_workspace"
        } else {
            "none"
        };
        let requested_workspace = args_obj
            .get("workspace")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut workspaces = Vec::<Value>::new();
        for row in rows {
            let ws_id = match WorkspaceId::try_new(row.workspace.clone()) {
                Ok(v) => v,
                Err(_) => {
                    // Corrupt/legacy data: keep listing but skip binding lookup.
                    workspaces.push(json!({
                        "workspace": row.workspace,
                        "created_at_ms": row.created_at_ms,
                        "project_guard": row.project_guard,
                        "bound_path": null,
                        "bound_paths": null
                    }));
                    continue;
                }
            };

            let binding = match self.store.workspace_path_summary_get(&ws_id) {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let (bound_path, bound_paths) = match binding {
                Some((path, last_used_at_ms, count)) => (
                    Value::String(path),
                    json!({ "count": count, "last_used_at_ms": last_used_at_ms }),
                ),
                None => (Value::Null, Value::Null),
            };

            workspaces.push(json!({
                "workspace": ws_id.as_str(),
                "created_at_ms": row.created_at_ms,
                "project_guard": row.project_guard,
                "bound_path": bound_path,
                "bound_paths": bound_paths
            }));
        }

        ai_ok(
            "workspace_list",
            json!({
                "selected_workspace": selected.clone(),
                "active_workspace": selected,
                "selected_workspace_source": selected_source,
                "requested_workspace": requested_workspace,
                "workspaces": workspaces,
                "count": workspaces.len(),
                "limit": limit,
                "offset": offset,
                "truncated": truncated_by_limit
            }),
        )
    }
}
