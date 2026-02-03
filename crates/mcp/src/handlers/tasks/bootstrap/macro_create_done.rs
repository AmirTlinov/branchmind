#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_macro_create_done(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
        let Some(steps_array) = steps_value.as_array() else {
            return ai_error("INVALID_INPUT", "steps must be an array");
        };
        if steps_array.len() != 1 {
            return ai_error(
                "INVALID_INPUT",
                "steps must contain exactly one step for macro_create_done",
            );
        }

        let bootstrap = self.tool_tasks_bootstrap(args.clone());
        if !bootstrap
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return bootstrap;
        }

        let task_id = match bootstrap
            .get("result")
            .and_then(|v| v.get("task"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
        {
            Some(v) => v.to_string(),
            None => return ai_error("STORE_ERROR", "bootstrap result missing task id"),
        };
        let task_revision = bootstrap
            .get("result")
            .and_then(|v| v.get("task"))
            .and_then(|v| v.get("revision"))
            .and_then(|v| v.as_i64());
        let step_path = bootstrap
            .get("result")
            .and_then(|v| v.get("steps"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if step_path.is_none() {
            return ai_error("STORE_ERROR", "bootstrap result missing step path");
        }

        let mut close_args = serde_json::Map::new();
        close_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        close_args.insert("task".to_string(), Value::String(task_id.clone()));
        close_args.insert(
            "path".to_string(),
            Value::String(step_path.clone().unwrap()),
        );
        close_args.insert(
            "checkpoints".to_string(),
            json!({ "criteria": { "confirmed": true }, "tests": { "confirmed": true } }),
        );
        if let Some(revision) = task_revision {
            close_args.insert(
                "expected_revision".to_string(),
                Value::Number(serde_json::Number::from(revision)),
            );
        }
        let close = self.tool_tasks_close_step(Value::Object(close_args));
        if !close
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return close;
        }

        let close_revision = close
            .get("result")
            .and_then(|v| v.get("revision"))
            .and_then(|v| v.as_i64());

        let mut complete_args = serde_json::Map::new();
        complete_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        complete_args.insert("task".to_string(), Value::String(task_id.clone()));
        complete_args.insert("status".to_string(), Value::String("DONE".to_string()));
        if let Some(revision) = close_revision {
            complete_args.insert(
                "expected_revision".to_string(),
                Value::Number(serde_json::Number::from(revision)),
            );
        }
        let complete = self.tool_tasks_complete(Value::Object(complete_args));
        if !complete
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return complete;
        }

        let mut warnings = Vec::new();
        if let Some(w) = bootstrap.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = close.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = complete.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "bootstrap": bootstrap.get("result").cloned().unwrap_or(Value::Null),
            "close": close.get("result").cloned().unwrap_or(Value::Null),
            "complete": complete.get("result").cloned().unwrap_or(Value::Null)
        });

        if warnings.is_empty() {
            ai_ok("tasks_macro_create_done", result)
        } else {
            ai_ok_with_warnings("tasks_macro_create_done", result, warnings, Vec::new())
        }
    }
}
