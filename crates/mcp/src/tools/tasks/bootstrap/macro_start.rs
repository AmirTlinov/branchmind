#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

const DEFAULT_PLAN_TITLE: &str = "Inbox";
const DEFAULT_TASK_TEMPLATE: &str = "basic-task";

impl McpServer {
    pub(crate) fn tool_tasks_macro_start(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let mut patched_args = args_obj.clone();
        let workspace = match require_workspace(&patched_args) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let workspace_label = workspace.as_str();
        let resume_max_chars = match optional_usize(args_obj, "resume_max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let view = match optional_string(args_obj, "view") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let has_plan = patched_args.get("plan").and_then(|v| v.as_str()).is_some();
        let has_parent = patched_args
            .get("parent")
            .and_then(|v| v.as_str())
            .is_some();
        let has_plan_title = patched_args
            .get("plan_title")
            .and_then(|v| v.as_str())
            .is_some();
        if !has_plan && !has_parent && !has_plan_title {
            match self.store.focus_get(&workspace) {
                Ok(Some(focus)) if focus.starts_with("PLAN-") => {
                    patched_args.insert("plan".to_string(), Value::String(focus));
                }
                Ok(Some(focus)) if focus.starts_with("TASK-") => {
                    match self.store.get_task(&workspace, &focus) {
                        Ok(Some(task)) => {
                            patched_args
                                .insert("plan".to_string(), Value::String(task.parent_plan_id));
                        }
                        Ok(None) => {}
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    }
                }
                Ok(_) => {}
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        }

        let has_plan = patched_args.get("plan").and_then(|v| v.as_str()).is_some();
        let has_parent = patched_args
            .get("parent")
            .and_then(|v| v.as_str())
            .is_some();
        let has_plan_title = patched_args
            .get("plan_title")
            .and_then(|v| v.as_str())
            .is_some();
        if !has_plan && !has_parent && !has_plan_title {
            match self
                .store
                .find_plan_id_by_title(&workspace, DEFAULT_PLAN_TITLE)
            {
                Ok(Some(id)) => {
                    patched_args.insert("plan".to_string(), Value::String(id));
                }
                Ok(None) => {
                    patched_args.insert(
                        "plan_title".to_string(),
                        Value::String(DEFAULT_PLAN_TITLE.to_string()),
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        }

        let has_template = patched_args
            .get("template")
            .and_then(|v| v.as_str())
            .is_some();
        let has_steps = patched_args.get("steps").is_some_and(|v| !v.is_null());
        if !has_template && !has_steps {
            patched_args.insert(
                "template".to_string(),
                Value::String(DEFAULT_TASK_TEMPLATE.to_string()),
            );
        }

        // Reasoning-first: principal tasks should seed a minimal frame card automatically,
        // without forcing agents to remember extra calls or verbose inputs.
        //
        // The seed is deterministic (derived from the user-provided task title/description)
        // and stays low-noise (one card).
        let template_id = patched_args.get("template").and_then(|v| v.as_str());
        let think_missing = patched_args.get("think").is_none_or(|v| v.is_null());
        if template_id == Some("principal-task") && think_missing {
            let task_title = patched_args
                .get("task_title")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let description = patched_args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut frame = serde_json::Map::new();
            if !task_title.trim().is_empty() {
                frame.insert(
                    "title".to_string(),
                    Value::String(task_title.trim().to_string()),
                );
            }
            if !description.trim().is_empty() {
                frame.insert(
                    "text".to_string(),
                    Value::String(description.trim().to_string()),
                );
            }
            if !frame.is_empty() {
                let mut think_obj = serde_json::Map::new();
                think_obj.insert("frame".to_string(), Value::Object(frame));
                patched_args.insert("think".to_string(), Value::Object(think_obj));
            }
        }

        let bootstrap = self.tool_tasks_bootstrap(Value::Object(patched_args.clone()));
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

        let plan_id = bootstrap
            .get("result")
            .and_then(|v| v.get("plan"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut resume_args = serde_json::Map::new();
        resume_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        resume_args.insert("task".to_string(), Value::String(task_id.clone()));
        if let Some(agent_id) = agent_id.as_deref() {
            resume_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
        }
        resume_args.insert(
            "view".to_string(),
            Value::String(view.unwrap_or_else(|| "smart".to_string())),
        );
        if let Some(max_chars) = resume_max_chars {
            resume_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        let resume = self.tool_tasks_resume_super(Value::Object(resume_args));
        if !resume
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return resume;
        }

        let mut warnings = Vec::new();
        if let Some(w) = bootstrap.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = resume.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let steps = bootstrap
            .get("result")
            .and_then(|v| v.get("steps"))
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let think_pipeline = bootstrap
            .get("result")
            .and_then(|v| v.get("think_pipeline"))
            .cloned()
            .filter(|v| !v.is_null());

        let mut result = json!({
            "task_id": task_id,
            "task_qualified_id": format!("{workspace_label}:{task_id}"),
            "steps": steps,
            "resume": resume.get("result").cloned().unwrap_or(Value::Null)
        });
        if let Some(plan_id) = plan_id.as_ref()
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("plan_id".to_string(), Value::String(plan_id.clone()));
            obj.insert(
                "plan_qualified_id".to_string(),
                Value::String(format!("{workspace_label}:{plan_id}")),
            );
        }
        if let Some(think_pipeline) = think_pipeline
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("think_pipeline".to_string(), think_pipeline);
        }

        if warnings.is_empty() {
            ai_ok("tasks_macro_start", result)
        } else {
            ai_ok_with_warnings("tasks_macro_start", result, warnings, Vec::new())
        }
    }
}
