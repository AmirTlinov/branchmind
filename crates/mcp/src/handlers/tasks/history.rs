#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_history(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match optional_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = args_obj
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);

        let rows = match self
            .store
            .ops_history_list(&workspace, task_id.as_deref(), limit)
        {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "history",
            json!({
                "workspace": workspace.as_str(),
                "operations": rows.into_iter().map(|row| {
                    json!({
                        "seq": row.seq,
                        "ts": ts_ms_to_rfc3339(row.ts_ms),
                        "ts_ms": row.ts_ms,
                        "task": row.task_id,
                        "path": row.path,
                        "intent": row.intent,
                        "payload": parse_json_or_string(&row.payload_json),
                        "before": row.before_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "after": row.after_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "undoable": row.undoable,
                        "undone": row.undone
                    })
                }).collect::<Vec<_>>()
            }),
        )
    }

    pub(crate) fn tool_tasks_undo(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match optional_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = self.store.ops_history_undo(&workspace, task_id.as_deref());
        match result {
            Ok((row, event)) => ai_ok(
                "undo",
                json!({
                    "workspace": workspace.as_str(),
                    "operation": {
                        "seq": row.seq,
                        "ts": ts_ms_to_rfc3339(row.ts_ms),
                        "ts_ms": row.ts_ms,
                        "task": row.task_id,
                        "path": row.path,
                        "intent": row.intent,
                        "payload": parse_json_or_string(&row.payload_json),
                        "before": row.before_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "after": row.after_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "undoable": row.undoable,
                        "undone": true
                    },
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
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_redo(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match optional_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = self.store.ops_history_redo(&workspace, task_id.as_deref());
        match result {
            Ok((row, event)) => ai_ok(
                "redo",
                json!({
                    "workspace": workspace.as_str(),
                    "operation": {
                        "seq": row.seq,
                        "ts": ts_ms_to_rfc3339(row.ts_ms),
                        "ts_ms": row.ts_ms,
                        "task": row.task_id,
                        "path": row.path,
                        "intent": row.intent,
                        "payload": parse_json_or_string(&row.payload_json),
                        "before": row.before_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "after": row.after_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "undoable": row.undoable,
                        "undone": false
                    },
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
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_batch(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let atomic = match optional_bool(args_obj, "atomic") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let compact = match optional_bool(args_obj, "compact") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let ops_value = args_obj.get("operations").cloned().unwrap_or(Value::Null);
        let Some(ops) = ops_value.as_array() else {
            return ai_error("INVALID_INPUT", "operations must be an array");
        };
        if ops.is_empty() {
            return ai_error("INVALID_INPUT", "operations must not be empty");
        }

        let mut responses = Vec::new();
        let mut applied_targets: Vec<String> = Vec::new();

        for (index, op) in ops.iter().enumerate() {
            let Some(op_obj) = op.as_object() else {
                return ai_error("INVALID_INPUT", "operations entries must be objects");
            };
            let tool_name = op_obj
                .get("tool")
                .or_else(|| op_obj.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if tool_name.is_empty() {
                return ai_error("INVALID_INPUT", "operation tool is required");
            }
            if tool_name == "tasks_batch" {
                return ai_error("INVALID_INPUT", "nested tasks_batch is not allowed");
            }
            if !batch_tool_allowed(tool_name) {
                return ai_error("INVALID_INPUT", "tool is not allowed in batch");
            }
            if atomic && !batch_tool_undoable(tool_name) {
                return ai_error("INVALID_INPUT", "tool is not undoable for atomic batch");
            }

            let args_value = op_obj
                .get("args")
                .or_else(|| op_obj.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let Some(mut op_args) = args_value.as_object().cloned() else {
                return ai_error("INVALID_INPUT", "operation args must be an object");
            };
            op_args
                .entry("workspace".to_string())
                .or_insert_with(|| Value::String(workspace.as_str().to_string()));

            let target_id = if atomic {
                match batch_target_id(&op_args) {
                    Some(value) => value,
                    None => {
                        return ai_error(
                            "INVALID_INPUT",
                            "atomic batch requires task or plan id in args",
                        );
                    }
                }
            } else {
                String::new()
            };

            let response = self.call_tool(tool_name, Value::Object(op_args));
            let ok = response
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !ok {
                let error_message = response
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("operation failed");
                if atomic {
                    let mut rollback_errors = Vec::new();
                    for target in applied_targets.into_iter().rev() {
                        if let Err(err) = self
                            .store
                            .ops_history_undo(&workspace, Some(target.as_str()))
                        {
                            rollback_errors.push(format_store_error(err));
                        }
                    }
                    let mut message =
                        format!("operation {index} ({tool_name}) failed: {error_message}");
                    if !rollback_errors.is_empty() {
                        message.push_str("; rollback failed: ");
                        message.push_str(&rollback_errors.join(", "));
                    }
                    return ai_error("BATCH_FAILED", &message);
                }
                return ai_error(
                    "BATCH_FAILED",
                    &format!("operation {index} ({tool_name}) failed: {error_message}"),
                );
            }

            if compact {
                if ok {
                    responses.push(json!({
                        "index": index,
                        "tool": tool_name,
                        "success": true
                    }));
                } else {
                    responses.push(json!({
                        "index": index,
                        "tool": tool_name,
                        "success": false,
                        "error": response.get("error").cloned().unwrap_or(Value::Null)
                    }));
                }
            } else {
                responses.push(json!({
                    "index": index,
                    "tool": tool_name,
                    "response": response
                }));
            }
            if atomic {
                applied_targets.push(target_id);
            }
        }

        ai_ok(
            "batch",
            json!({
                "workspace": workspace.as_str(),
                "atomic": atomic,
                "compact": compact,
                "operations": responses
            }),
        )
    }
}
