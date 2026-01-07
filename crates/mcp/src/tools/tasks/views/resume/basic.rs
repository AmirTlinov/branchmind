#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_resume(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let events_limit = args_obj
            .get("events_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(20);
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let explicit_target = args_obj
            .get("task")
            .and_then(|v| v.as_str())
            .or_else(|| args_obj.get("plan").and_then(|v| v.as_str()));

        let (target_id, kind, focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (focus, focus_previous, focus_restored) = if read_only {
            (focus, None, false)
        } else {
            match restore_focus_for_explicit_target(
                &mut self.store,
                &workspace,
                explicit_target,
                focus,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        };

        let context = match build_radar_context_with_options(
            &mut self.store,
            &workspace,
            &target_id,
            kind,
            read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut events = if events_limit == 0 {
            Vec::new()
        } else {
            match self
                .store
                .list_events_for_task(&workspace, &target_id, events_limit)
            {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };
        events.reverse();
        sort_events_by_seq(&mut events);

        let mut steps_detail: Option<Value> = None;
        if kind == TaskKind::Task {
            let steps = match self
                .store
                .list_task_steps(&workspace, &target_id, None, 200)
            {
                Ok(v) => v,
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let mut out = Vec::with_capacity(steps.len());
            for step in steps {
                let detail = match self.store.step_detail(
                    &workspace,
                    &target_id,
                    Some(step.step_id.as_str()),
                    None,
                ) {
                    Ok(v) => v,
                    Err(StoreError::StepNotFound) => {
                        return ai_error("UNKNOWN_ID", "Step not found");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                out.push(json!({
                    "step_id": detail.step_id,
                    "path": detail.path,
                    "title": detail.title,
                    "completed": detail.completed,
                    "created_at_ms": step.created_at_ms,
                    "updated_at_ms": step.updated_at_ms,
                    "completed_at_ms": step.completed_at_ms,
                    "criteria_confirmed": detail.criteria_confirmed,
                    "tests_confirmed": detail.tests_confirmed,
                    "security_confirmed": detail.security_confirmed,
                    "perf_confirmed": detail.perf_confirmed,
                    "docs_confirmed": detail.docs_confirmed,
                    "blocked": detail.blocked,
                    "block_reason": detail.block_reason,
                    "success_criteria": detail.success_criteria,
                    "tests": detail.tests,
                    "blockers": detail.blockers
                }));
            }
            steps_detail = Some(json!(out));
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "requested": {
                "task": args_obj.get("task").cloned().unwrap_or(Value::Null),
                "plan": args_obj.get("plan").cloned().unwrap_or(Value::Null)
            },
            "focus": focus,
            "target": context.target,
            "reasoning_ref": context.reasoning_ref,
            "radar": context.radar,
            "timeline": {
                "limit": events_limit,
                "events": events_to_json(events)
            }
        });
        if focus_restored && let Some(obj) = result.as_object_mut() {
            obj.insert("focus_restored".to_string(), Value::Bool(true));
            obj.insert(
                "focus_previous".to_string(),
                focus_previous.map(Value::String).unwrap_or(Value::Null),
            );
        }
        if let Some(steps) = steps_detail.or(context.steps)
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("steps".to_string(), steps);
        }

        ai_ok("resume", result)
    }
}
