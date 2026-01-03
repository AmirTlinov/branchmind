#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_edit(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let (task_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj)
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let title = match optional_non_null_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let description = match optional_nullable_string(args_obj, "description") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let context = match optional_nullable_string(args_obj, "context") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let priority = match optional_string(args_obj, "priority") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let new_domain = match optional_nullable_string(args_obj, "new_domain") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags = match optional_string_array(args_obj, "tags") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let depends_on = match optional_string_array(args_obj, "depends_on") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let contract = match optional_nullable_string(args_obj, "contract") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let contract_json = match optional_nullable_object_as_json_string(args_obj, "contract_data")
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        match kind {
            TaskKind::Plan => {
                if new_domain.is_some() {
                    return ai_error("INVALID_INPUT", "new_domain is not valid for kind=plan");
                }
                if title.is_none()
                    && description.is_none()
                    && context.is_none()
                    && priority.is_none()
                    && tags.is_none()
                    && depends_on.is_none()
                    && contract.is_none()
                    && contract_json.is_none()
                {
                    return ai_error("INVALID_INPUT", "no fields to edit");
                }
            }
            TaskKind::Task => {
                if contract.is_some() || contract_json.is_some() {
                    return ai_error(
                        "INVALID_INPUT",
                        "contract fields are not valid for kind=task",
                    );
                }
                if title.is_none()
                    && description.is_none()
                    && context.is_none()
                    && priority.is_none()
                    && new_domain.is_none()
                    && tags.is_none()
                    && depends_on.is_none()
                {
                    return ai_error("INVALID_INPUT", "no fields to edit");
                }
            }
        }

        let mut patch = serde_json::Map::new();
        if let Some(ref value) = title {
            patch.insert("title".to_string(), Value::String(value.clone()));
        }

        match kind {
            TaskKind::Plan => {
                if let Some(ref value) = description {
                    patch.insert(
                        "description".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref value) = context {
                    patch.insert(
                        "context".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref value) = priority {
                    patch.insert("priority".to_string(), Value::String(value.clone()));
                }
                if let Some(ref items) = tags {
                    patch.insert(
                        "tags".to_string(),
                        Value::Array(items.iter().map(|v| Value::String(v.clone())).collect()),
                    );
                }
                if let Some(ref items) = depends_on {
                    patch.insert(
                        "depends_on".to_string(),
                        Value::Array(items.iter().map(|v| Value::String(v.clone())).collect()),
                    );
                }
                if let Some(ref value) = contract {
                    patch.insert(
                        "contract".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref value) = contract_json {
                    patch.insert(
                        "contract_data".to_string(),
                        match value {
                            Some(raw) => parse_json_or_string(raw),
                            None => Value::Null,
                        },
                    );
                }
            }
            TaskKind::Task => {
                if let Some(ref value) = description {
                    patch.insert(
                        "description".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref value) = context {
                    patch.insert(
                        "context".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref value) = priority {
                    patch.insert("priority".to_string(), Value::String(value.clone()));
                }
                if let Some(ref value) = new_domain {
                    patch.insert(
                        "domain".to_string(),
                        match value {
                            Some(v) => Value::String(v.clone()),
                            None => Value::Null,
                        },
                    );
                }
                if let Some(ref items) = tags {
                    patch.insert(
                        "tags".to_string(),
                        Value::Array(items.iter().map(|v| Value::String(v.clone())).collect()),
                    );
                }
                if let Some(ref items) = depends_on {
                    patch.insert(
                        "depends_on".to_string(),
                        Value::Array(items.iter().map(|v| Value::String(v.clone())).collect()),
                    );
                }
            }
        }

        let event_type = format!("{}_edited", kind.as_str());
        let event_payload_json = json!({
            "kind": kind.as_str(),
            "patch": Value::Object(patch),
        })
        .to_string();

        let result = match kind {
            TaskKind::Plan => self.store.edit_plan(
                &workspace,
                bm_storage::PlanEditRequest {
                    id: task_id.clone(),
                    expected_revision,
                    title,
                    description,
                    context,
                    priority,
                    tags,
                    depends_on,
                    contract,
                    contract_json,
                    event_type: event_type.clone(),
                    event_payload_json,
                },
            ),
            TaskKind::Task => self.store.edit_task(
                &workspace,
                bm_storage::TaskEditRequest {
                    id: task_id.clone(),
                    expected_revision,
                    title,
                    description,
                    context,
                    priority,
                    domain: new_domain,
                    phase: None,
                    component: None,
                    assignee: None,
                    tags,
                    depends_on,
                    event_type: event_type.clone(),
                    event_payload_json,
                },
            ),
        };

        match result {
            Ok((revision, event)) => ai_ok(
                "edit",
                json!({
                    "id": task_id,
                    "kind": kind.as_str(),
                    "revision": revision,
                    "event": {
                        "event_id": event.event_id(),
                        "ts": ts_ms_to_rfc3339(event.ts_ms),
                        "ts_ms": event.ts_ms,
                        "task_id": event.task_id,
                        "path": event.path,
                        "type": event.event_type,
                        "payload": parse_json_or_string(&event.payload_json)
                    }
                }),
            ),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
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
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
