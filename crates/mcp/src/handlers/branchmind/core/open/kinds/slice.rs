#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_slice(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    slice_id: &str,
    include_content: bool,
) -> Result<Value, Value> {
    if !server.slice_plans_v1_enabled {
        return Err(ai_error_with(
            "FEATURE_DISABLED",
            "slice_plans_v1 is disabled",
            Some("Enable via --slice-plans-v1 (or env BRANCHMIND_SLICE_PLANS_V1=1)."),
            Vec::new(),
        ));
    }

    let binding = match server.store.plan_slice_get_by_slice_id(workspace, slice_id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(ai_error(
                "UNKNOWN_ID",
                "Unknown slice_id (no plan_slices binding)",
            ));
        }
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let mut out = json!({
        "workspace": workspace.as_str(),
        "kind": "slice",
        "id": binding.slice_id.clone(),
        "slice": {
            "plan_id": binding.plan_id.clone(),
            "slice_id": binding.slice_id.clone(),
            "slice_task_id": binding.slice_task_id.clone(),
            "title": binding.title.clone(),
            "objective": binding.objective.clone(),
            "status": binding.status.clone(),
            "budgets_json": binding.budgets_json.clone(),
            "created_at_ms": binding.created_at_ms,
            "updated_at_ms": binding.updated_at_ms
        },
        "truncated": false
    });

    let mut actions = Vec::<Value>::new();
    actions.push(json!({
        "op": "call",
        "cmd": "tasks.slice.open",
        "reason": "Open slice plan tree (full spec + step tree).",
        "priority": "medium",
        "budget_profile": "portal",
        "args": {
            "workspace": workspace.as_str(),
            "slice_id": binding.slice_id.clone()
        }
    }));
    actions.push(json!({
        "op": "call",
        "cmd": "tasks.slice.validate",
        "reason": "Validate slice plan structure (fail-closed).",
        "priority": "high",
        "budget_profile": "portal",
        "args": {
            "workspace": workspace.as_str(),
            "slice_id": binding.slice_id.clone(),
            "policy": "fail_closed"
        }
    }));
    actions.push(json!({
        "op": "call",
        "cmd": "jobs.macro.dispatch.scout",
        "reason": "Start slice scout (bounded context pack).",
        "priority": "high",
        "budget_profile": "portal",
        "args": {
            "workspace": workspace.as_str(),
            "task": binding.plan_id.clone(),
            "anchor": format!("a:{}", binding.slice_id.to_ascii_lowercase()),
            "slice_id": binding.slice_id.clone(),
            "objective": binding.objective.clone(),
            "executor": "claude_code",
            "model": "haiku",
            "executor_profile": "deep",
            "quality_profile": "flagship"
        }
    }));

    if include_content {
        let slice_task = match server.store.get_task(workspace, &binding.slice_task_id) {
            Ok(Some(v)) => v,
            Ok(None) => return Err(ai_error("UNKNOWN_ID", "Unknown slice_task_id")),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        let spec = match crate::support::parse_slice_plan_spec_from_task_context(
            slice_task.context.as_deref(),
        ) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Err(ai_error(
                    "PRECONDITION_FAILED",
                    "slice_task_id missing slice_plan_spec JSON in context",
                ));
            }
            Err(resp) => return Err(resp),
        };
        let steps = match server
            .store
            .list_task_steps(workspace, &binding.slice_task_id, None, 200)
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| {
                    json!({
                        "step_id": row.step_id,
                        "path": row.path,
                        "title": row.title,
                        "completed": row.completed,
                        "criteria_confirmed": row.criteria_confirmed,
                        "tests_confirmed": row.tests_confirmed,
                        "blocked": row.blocked,
                        "block_reason": row.block_reason
                    })
                })
                .collect::<Vec<_>>(),
            Err(StoreError::StepNotFound) => Vec::new(),
            Err(StoreError::InvalidInput(msg)) => {
                return Err(ai_error("INVALID_INPUT", msg));
            }
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        if let Some(obj) = out.as_object_mut() {
            obj.insert(
                "slice_task".to_string(),
                json!({
                    "id": slice_task.id,
                    "title": slice_task.title,
                    "revision": slice_task.revision,
                    "status": slice_task.status,
                    "updated_at_ms": slice_task.updated_at_ms
                }),
            );
            obj.insert("slice_plan_spec".to_string(), spec.to_json());
            obj.insert("steps".to_string(), Value::Array(steps));
        }
    }
    if let Some(obj) = out.as_object_mut() {
        obj.insert("actions".to_string(), Value::Array(actions));
    }
    Ok(out)
}
