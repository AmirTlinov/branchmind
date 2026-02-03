#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct AppliedTaskDetailOps {
    pub patch: bm_storage::TaskDetailPatch,
    pub fields: Vec<String>,
}

pub(super) fn apply_task_detail_ops(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    kind: TaskKind,
    task_id: &str,
    ops: &[Value],
) -> Result<AppliedTaskDetailOps, Value> {
    let mut patch = bm_storage::TaskDetailPatch {
        title: None,
        description: None,
        context: None,
        priority: None,
        contract: None,
        contract_json: None,
        domain: None,
        phase: None,
        component: None,
        assignee: None,
        tags: None,
        depends_on: None,
    };
    let mut tags: Option<Vec<String>> = None;
    let mut depends: Option<Vec<String>> = None;
    let mut fields: Vec<String> = Vec::new();

    for op_value in ops {
        let Some(op_obj) = op_value.as_object() else {
            return Err(ai_error("INVALID_INPUT", "ops entries must be objects"));
        };
        let op_name = require_string(op_obj, "op")?;
        let field = require_string(op_obj, "field")?;
        let value = op_obj.get("value");

        match field.as_str() {
            "title" => {
                if op_name != "set" {
                    return Err(ai_error("INVALID_INPUT", "title supports only set"));
                }
                let Some(Value::String(v)) = value else {
                    return Err(ai_error("INVALID_INPUT", "title must be a string"));
                };
                patch.title = Some(v.clone());
                push_unique_field(&mut fields, "title");
            }
            "description" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "description must be string or null",
                                ));
                            }
                        };
                        patch.description = Some(next);
                    }
                    "unset" => patch.description = Some(None),
                    _ => {
                        return Err(ai_error("INVALID_INPUT", "description supports set/unset"));
                    }
                }
                push_unique_field(&mut fields, "description");
            }
            "context" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "context must be string or null",
                                ));
                            }
                        };
                        patch.context = Some(next);
                    }
                    "unset" => patch.context = Some(None),
                    _ => {
                        return Err(ai_error("INVALID_INPUT", "context supports set/unset"));
                    }
                }
                push_unique_field(&mut fields, "context");
            }
            "priority" => {
                if op_name != "set" {
                    return Err(ai_error("INVALID_INPUT", "priority supports only set"));
                }
                let Some(Value::String(v)) = value else {
                    return Err(ai_error("INVALID_INPUT", "priority must be a string"));
                };
                patch.priority = Some(v.clone());
                push_unique_field(&mut fields, "priority");
            }
            "contract" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "contract must be string or null",
                                ));
                            }
                        };
                        patch.contract = Some(next);
                    }
                    "unset" => patch.contract = Some(None),
                    _ => {
                        return Err(ai_error("INVALID_INPUT", "contract supports set/unset"));
                    }
                }
                push_unique_field(&mut fields, "contract");
            }
            "contract_data" => {
                match op_name.as_str() {
                    "set" => {
                        let Some(v) = value else {
                            return Err(ai_error("INVALID_INPUT", "contract_data requires value"));
                        };
                        if v.is_null() {
                            patch.contract_json = Some(None);
                        } else {
                            patch.contract_json = Some(Some(v.to_string()));
                        }
                    }
                    "unset" => patch.contract_json = Some(None),
                    _ => {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            "contract_data supports set/unset",
                        ));
                    }
                }
                push_unique_field(&mut fields, "contract_data");
            }
            "domain" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "domain must be string or null",
                                ));
                            }
                        };
                        patch.domain = Some(next);
                    }
                    "unset" => patch.domain = Some(None),
                    _ => return Err(ai_error("INVALID_INPUT", "domain supports set/unset")),
                }
                push_unique_field(&mut fields, "domain");
            }
            "phase" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "phase must be string or null",
                                ));
                            }
                        };
                        patch.phase = Some(next);
                    }
                    "unset" => patch.phase = Some(None),
                    _ => return Err(ai_error("INVALID_INPUT", "phase supports set/unset")),
                }
                push_unique_field(&mut fields, "phase");
            }
            "component" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "component must be string or null",
                                ));
                            }
                        };
                        patch.component = Some(next);
                    }
                    "unset" => patch.component = Some(None),
                    _ => {
                        return Err(ai_error("INVALID_INPUT", "component supports set/unset"));
                    }
                }
                push_unique_field(&mut fields, "component");
            }
            "assignee" => {
                match op_name.as_str() {
                    "set" => {
                        let next = match value {
                            Some(Value::Null) => None,
                            Some(Value::String(v)) => Some(v.clone()),
                            _ => {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "assignee must be string or null",
                                ));
                            }
                        };
                        patch.assignee = Some(next);
                    }
                    "unset" => patch.assignee = Some(None),
                    _ => {
                        return Err(ai_error("INVALID_INPUT", "assignee supports set/unset"));
                    }
                }
                push_unique_field(&mut fields, "assignee");
            }
            "tags" => {
                let mut list =
                    get_or_load_task_list(server, workspace, kind, task_id, "tags", tags.take())?;
                apply_list_op(&mut list, &op_name, value, "tags")?;
                tags = Some(list);
                push_unique_field(&mut fields, "tags");
            }
            "depends_on" => {
                let mut list = get_or_load_task_list(
                    server,
                    workspace,
                    kind,
                    task_id,
                    "depends_on",
                    depends.take(),
                )?;
                apply_list_op(&mut list, &op_name, value, "depends_on")?;
                depends = Some(list);
                push_unique_field(&mut fields, "depends_on");
            }
            _ => return Err(ai_error("INVALID_INPUT", "unknown task_detail field")),
        }
    }

    if let Some(list) = tags {
        patch.tags = Some(list);
    }
    if let Some(list) = depends {
        patch.depends_on = Some(list);
    }

    Ok(AppliedTaskDetailOps { patch, fields })
}

fn push_unique_field(fields: &mut Vec<String>, name: &str) {
    if !fields.iter().any(|f| f == name) {
        fields.push(name.to_string());
    }
}

fn get_or_load_task_list(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    kind: TaskKind,
    task_id: &str,
    field: &str,
    current: Option<Vec<String>>,
) -> Result<Vec<String>, Value> {
    if let Some(list) = current {
        return Ok(list);
    }
    match server
        .store
        .task_items_list(workspace, kind.as_str(), task_id, field)
    {
        Ok(v) => Ok(v),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}
