#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_macro_finish(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let handoff_max_chars = match optional_usize(args_obj, "handoff_max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let complete = self.tool_tasks_complete(args.clone());
        if !complete
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return complete;
        }

        let task_id = match complete
            .get("result")
            .and_then(|v| v.get("task"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
        {
            Some(v) => v.to_string(),
            None => return ai_error("STORE_ERROR", "complete result missing task id"),
        };

        let mut handoff_args = serde_json::Map::new();
        handoff_args.insert(
            "workspace".to_string(),
            args_obj.get("workspace").cloned().unwrap_or(Value::Null),
        );
        handoff_args.insert("task".to_string(), Value::String(task_id.clone()));
        handoff_args.insert("read_only".to_string(), Value::Bool(true));
        if let Some(max_chars) = handoff_max_chars {
            handoff_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        let handoff = self.tool_tasks_handoff(Value::Object(handoff_args));
        if !handoff
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return handoff;
        }

        let mut warnings = Vec::new();
        if let Some(w) = complete.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = handoff.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let result = json!({
            "task": task_id,
            "status": complete
                .get("result")
                .and_then(|v| v.get("task"))
                .and_then(|v| v.get("status"))
                .cloned()
                .unwrap_or(Value::Null),
            "handoff": handoff.get("result").cloned().unwrap_or(Value::Null)
        });

        if warnings.is_empty() {
            ai_ok("tasks_macro_finish", result)
        } else {
            ai_ok_with_warnings("tasks_macro_finish", result, warnings, Vec::new())
        }
    }
}
