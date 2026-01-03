#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_focus_get(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        match self.store.focus_get(&workspace) {
            Ok(focus) => ai_ok(
                "focus_get",
                json!({
                    "workspace": workspace.as_str(),
                    "focus": focus
                }),
            ),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_focus_set(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task = args_obj.get("task").and_then(|v| v.as_str());
        let plan = args_obj.get("plan").and_then(|v| v.as_str());
        if task.is_some() && plan.is_some() {
            return ai_error(
                "INVALID_INPUT",
                "provide task or plan, not both; fix: task=\"TASK-001\"",
            );
        }
        let target_id = match task.or(plan) {
            Some(v) => v.to_string(),
            None => return ai_error("INVALID_INPUT", "task is required; fix: task=\"TASK-001\""),
        };
        if !target_id.starts_with("PLAN-") && !target_id.starts_with("TASK-") {
            return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-");
        }

        let prev = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        if let Err(err) = self.store.focus_set(&workspace, &target_id) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        ai_ok(
            "focus_set",
            json!({
                "workspace": workspace.as_str(),
                "previous": prev,
                "focus": target_id
            }),
        )
    }

    pub(crate) fn tool_tasks_focus_clear(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let prev = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let cleared = match self.store.focus_clear(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "focus_clear",
            json!({
                "workspace": workspace.as_str(),
                "previous": prev,
                "cleared": cleared
            }),
        )
    }
}
