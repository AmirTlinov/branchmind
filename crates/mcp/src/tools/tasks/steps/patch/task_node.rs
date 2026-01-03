#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn handle_task_node_patch(
    server: &mut McpServer,
    args_obj: &serde_json::Map<String, Value>,
    workspace: &WorkspaceId,
    task_id: String,
    expected_revision: Option<i64>,
    ops: &[Value],
) -> Value {
    let node_id = match optional_string(args_obj, "task_node_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let path = args_obj
        .get("path")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    if node_id.is_none() && path.is_none() {
        return ai_error("INVALID_INPUT", "task_node_id or path is required");
    }
    let (parent_path, ordinal) = if node_id.is_none() {
        let Some(path) = path else {
            return ai_error("INVALID_INPUT", "path is required");
        };
        match parse_task_node_path(&path) {
            Ok(v) => v,
            Err(resp) => return resp,
        }
    } else {
        (StepPath::root(0), 0)
    };

    let detail = match server.store.task_node_detail(
        workspace,
        &task_id,
        node_id.as_deref(),
        if node_id.is_some() {
            None
        } else {
            Some(&parent_path)
        },
        if node_id.is_some() {
            None
        } else {
            Some(ordinal)
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Task node not found"),
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let mut patch = bm_storage::TaskNodePatch {
        title: None,
        status: None,
        status_manual: None,
        priority: None,
        blocked: None,
        description: None,
        context: None,
        blockers: None,
        dependencies: None,
        next_steps: None,
        problems: None,
        risks: None,
        success_criteria: None,
    };
    let mut blockers_list = detail.blockers.clone();
    let mut dependencies_list = detail.dependencies.clone();
    let mut next_steps_list = detail.next_steps.clone();
    let mut problems_list = detail.problems.clone();
    let mut risks_list = detail.risks.clone();
    let mut success_list = detail.success_criteria.clone();

    for op_value in ops {
        let Some(op_obj) = op_value.as_object() else {
            return ai_error("INVALID_INPUT", "ops entries must be objects");
        };
        let op_name = match require_string(op_obj, "op") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let field = match require_string(op_obj, "field") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let value = op_obj.get("value");

        match field.as_str() {
            "title" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "title supports only set");
                }
                let Some(Value::String(v)) = value else {
                    return ai_error("INVALID_INPUT", "title must be a string");
                };
                patch.title = Some(v.clone());
            }
            "status" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "status supports only set");
                }
                let Some(Value::String(v)) = value else {
                    return ai_error("INVALID_INPUT", "status must be a string");
                };
                patch.status = Some(v.clone());
            }
            "status_manual" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "status_manual supports only set");
                }
                let Some(Value::Bool(v)) = value else {
                    return ai_error("INVALID_INPUT", "status_manual must be boolean");
                };
                patch.status_manual = Some(*v);
            }
            "priority" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "priority supports only set");
                }
                let Some(Value::String(v)) = value else {
                    return ai_error("INVALID_INPUT", "priority must be a string");
                };
                patch.priority = Some(v.clone());
            }
            "blocked" => {
                if op_name != "set" {
                    return ai_error("INVALID_INPUT", "blocked supports only set");
                }
                let Some(Value::Bool(v)) = value else {
                    return ai_error("INVALID_INPUT", "blocked must be boolean");
                };
                patch.blocked = Some(*v);
            }
            "description" => match op_name.as_str() {
                "set" => {
                    let next = match value {
                        Some(Value::Null) => None,
                        Some(Value::String(v)) => Some(v.clone()),
                        _ => {
                            return ai_error("INVALID_INPUT", "description must be string or null");
                        }
                    };
                    patch.description = Some(next);
                }
                "unset" => patch.description = Some(None),
                _ => {
                    return ai_error("INVALID_INPUT", "description supports set/unset");
                }
            },
            "context" => match op_name.as_str() {
                "set" => {
                    let next = match value {
                        Some(Value::Null) => None,
                        Some(Value::String(v)) => Some(v.clone()),
                        _ => {
                            return ai_error("INVALID_INPUT", "context must be string or null");
                        }
                    };
                    patch.context = Some(next);
                }
                "unset" => patch.context = Some(None),
                _ => return ai_error("INVALID_INPUT", "context supports set/unset"),
            },
            "blockers" => {
                if let Err(resp) = apply_list_op(&mut blockers_list, &op_name, value, "blockers") {
                    return resp;
                }
                patch.blockers = Some(blockers_list.clone());
            }
            "dependencies" => {
                if let Err(resp) =
                    apply_list_op(&mut dependencies_list, &op_name, value, "dependencies")
                {
                    return resp;
                }
                patch.dependencies = Some(dependencies_list.clone());
            }
            "next_steps" => {
                if let Err(resp) =
                    apply_list_op(&mut next_steps_list, &op_name, value, "next_steps")
                {
                    return resp;
                }
                patch.next_steps = Some(next_steps_list.clone());
            }
            "problems" => {
                if let Err(resp) = apply_list_op(&mut problems_list, &op_name, value, "problems") {
                    return resp;
                }
                patch.problems = Some(problems_list.clone());
            }
            "risks" => {
                if let Err(resp) = apply_list_op(&mut risks_list, &op_name, value, "risks") {
                    return resp;
                }
                patch.risks = Some(risks_list.clone());
            }
            "success_criteria" => {
                if let Err(resp) =
                    apply_list_op(&mut success_list, &op_name, value, "success_criteria")
                {
                    return resp;
                }
                patch.success_criteria = Some(success_list.clone());
            }
            _ => return ai_error("INVALID_INPUT", "unknown task node field"),
        }
    }

    let result = server.store.task_node_patch(
        workspace,
        bm_storage::TaskNodePatchRequest {
            task_id: task_id.clone(),
            expected_revision,
            selector: bm_storage::TaskNodeSelector {
                node_id: node_id.clone(),
                parent_path: if node_id.is_some() {
                    None
                } else {
                    Some(parent_path.clone())
                },
                ordinal: if node_id.is_some() {
                    None
                } else {
                    Some(ordinal)
                },
            },
            patch,
            record_undo: true,
        },
    );

    match result {
        Ok(out) => ai_ok(
            "patch",
            json!({
                "task": task_id,
                "revision": out.task_revision,
                "node": { "node_id": out.node.node_id, "path": out.node.path },
                "event": {
                    "event_id": out.event.event_id(),
                    "ts": ts_ms_to_rfc3339(out.event.ts_ms),
                    "ts_ms": out.event.ts_ms,
                    "task_id": out.event.task_id,
                    "path": out.event.path,
                    "type": out.event.event_type,
                    "payload": parse_json_or_string(&out.event.payload_json)
                }
            }),
        ),
        Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Task node not found"),
        Err(StoreError::RevisionMismatch { expected, actual }) => ai_error_with(
            "REVISION_MISMATCH",
            &format!("expected={expected} actual={actual}"),
            Some("Refresh the current revision and retry with expected_revision."),
            vec![suggest_call(
                "tasks_context",
                "Refresh current revisions for this workspace.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            )],
        ),
        Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
        Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
    }
}
