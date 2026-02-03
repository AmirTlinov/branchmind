#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_mirror(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit = args_obj
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let result = match kind {
            TaskKind::Plan => {
                if path.is_some() {
                    return ai_error("INVALID_INPUT", "path is not supported for plan mirror");
                }
                let plan = match self.store.get_plan(&workspace, &target_id) {
                    Ok(Some(p)) => p,
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let checklist = match self.store.plan_checklist_get(&workspace, &target_id) {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let tasks = match self
                    .store
                    .list_tasks_for_plan(&workspace, &target_id, limit, 0)
                {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                json!({
                    "scope": { "id": plan.id, "kind": "plan" },
                    "plan": {
                        "id": plan.id,
                        "title": plan.title,
                        "revision": plan.revision,
                        "status": plan.status
                    },
                    "checklist": {
                        "doc": checklist.doc,
                        "current": checklist.current,
                        "steps": checklist.steps
                    },
                    "tasks": tasks.into_iter().map(|t| json!({
                        "id": t.id,
                        "title": t.title,
                        "status": t.status,
                        "revision": t.revision
                    })).collect::<Vec<_>>()
                })
            }
            TaskKind::Task => {
                let steps =
                    match self
                        .store
                        .list_task_steps(&workspace, &target_id, path.as_ref(), limit)
                    {
                        Ok(v) => v,
                        Err(StoreError::StepNotFound) => {
                            return ai_error("UNKNOWN_ID", "Step not found");
                        }
                        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                json!({
                    "scope": { "id": target_id, "kind": "task" },
                    "steps": steps.into_iter().map(|s| json!({
                        "step_id": s.step_id,
                        "path": s.path,
                        "title": s.title,
                        "completed": s.completed,
                        "criteria_confirmed": s.criteria_confirmed,
                        "tests_confirmed": s.tests_confirmed,
                        "security_confirmed": s.security_confirmed,
                        "perf_confirmed": s.perf_confirmed,
                        "docs_confirmed": s.docs_confirmed,
                        "blocked": s.blocked,
                        "block_reason": s.block_reason
                    })).collect::<Vec<_>>()
                })
            }
        };

        ai_ok("mirror", result)
    }
}
