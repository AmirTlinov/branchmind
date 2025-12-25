#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use bm_storage::{SqliteStore, StoreError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const MCP_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "branchmind-rust-mcp";
const SERVER_VERSION: &str = "0.1.0";

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    #[serde(rename = "jsonrpc")]
    _jsonrpc: Option<String>,
    method: String,
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    params: Option<Value>,
}

fn json_rpc_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_text_content(payload: &Value) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("text".to_string())),
            (
                "text".to_string(),
                Value::String(serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string())),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

struct McpServer {
    initialized: bool,
    store: SqliteStore,
}

impl McpServer {
    fn new(store: SqliteStore) -> Self {
        Self {
            initialized: false,
            store,
        }
    }

    fn handle(&mut self, request: JsonRpcRequest) -> Option<Value> {
        let method = request.method.as_str();

        if method == "initialize" {
            return Some(json_rpc_response(
                request.id,
                json!({
                    "protocolVersion": MCP_VERSION,
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                    "capabilities": { "tools": {} }
                }),
            ));
        }

        if !self.initialized && method != "notifications/initialized" {
            return Some(json_rpc_error(request.id, -32002, "Server not initialized"));
        }

        if method == "notifications/initialized" {
            self.initialized = true;
            return None;
        }

        if method == "ping" {
            return Some(json_rpc_response(request.id, json!({})));
        }

        if method == "tools/list" {
            return Some(json_rpc_response(request.id, json!({ "tools": tool_definitions() })));
        }

        if method == "tools/call" {
            let Some(params) = request.params else {
                return Some(json_rpc_error(request.id, -32602, "params must be an object"));
            };
            let Some(params_obj) = params.as_object() else {
                return Some(json_rpc_error(request.id, -32602, "params must be an object"));
            };

            let tool_name = params_obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params_obj.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let response_body = self.call_tool(tool_name, args);

            return Some(json_rpc_response(
                request.id,
                json!({
                    "content": [tool_text_content(&response_body)],
                    "isError": !response_body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                }),
            ));
        }

        Some(json_rpc_error(request.id, -32601, &format!("Method not found: {method}")))
    }

    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        match name {
            "tasks_create" => self.tool_tasks_create(args),
            "tasks_decompose" => self.tool_tasks_decompose(args),
            "tasks_define" => self.tool_tasks_define(args),
            "tasks_note" => self.tool_tasks_note(args),
            "tasks_verify" => self.tool_tasks_verify(args),
            "tasks_done" => self.tool_tasks_done(args),
            "tasks_edit" => self.tool_tasks_edit(args),
            "tasks_context" => self.tool_tasks_context(args),
            "tasks_delta" => self.tool_tasks_delta(args),
            "tasks_focus_get" => self.tool_tasks_focus_get(args),
            "tasks_focus_set" => self.tool_tasks_focus_set(args),
            "tasks_focus_clear" => self.tool_tasks_focus_clear(args),
            "tasks_radar" => self.tool_tasks_radar(args),
            "storage" => self.tool_storage(args),
            _ => ai_error("UNKNOWN_TOOL", &format!("Unknown tool: {name}")),
        }
    }

    fn tool_storage(&mut self, _args: Value) -> Value {
        ai_ok(
            "storage",
            json!({
                "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
            }),
        )
    }

    fn tool_tasks_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let title = match require_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let parent = args_obj.get("parent").and_then(|v| v.as_str()).map(|s| s.to_string());
        let kind = parse_kind(args_obj.get("kind").and_then(|v| v.as_str()), parent.is_some());

        let description = args_obj.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
        let contract = args_obj.get("contract").and_then(|v| v.as_str()).map(|s| s.to_string());
        let contract_json = args_obj.get("contract_data").map(|v| v.to_string());

        if args_obj.get("steps").is_some() {
            return ai_error("NOT_IMPLEMENTED", "steps are not implemented in v0 skeleton");
        }

        let event_type = match kind {
            TaskKind::Plan => "plan_created",
            TaskKind::Task => "task_created",
        }
        .to_string();

        let event_payload_json = json!({
            "kind": kind.as_str(),
            "title": title.clone(),
            "parent": parent.clone(),
        })
        .to_string();

        match self.store.create(
            &workspace,
            kind,
            title,
            parent.clone(),
            description,
            contract,
            contract_json,
            event_type.clone(),
            event_payload_json,
        ) {
            Ok((id, revision, event)) => ai_ok(
                "create",
                json!({
                    "id": id,
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
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_decompose(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_path = match optional_step_path(args_obj, "parent") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
        let steps_array = steps_value
            .as_array()
            .ok_or_else(|| ai_error("INVALID_INPUT", "steps must be an array"));
        let Ok(steps_array) = steps_array else {
            return steps_array.err().unwrap();
        };

        if steps_array.is_empty() {
            return ai_error("INVALID_INPUT", "steps must not be empty");
        }

        let mut steps = Vec::with_capacity(steps_array.len());
        for step in steps_array {
            let Some(obj) = step.as_object() else {
                return ai_error("INVALID_INPUT", "steps[] items must be objects");
            };
            let title = match require_string(obj, "title") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let criteria_value = obj.get("success_criteria").cloned().unwrap_or(Value::Null);
            let Some(criteria_array) = criteria_value.as_array() else {
                return ai_error("INVALID_INPUT", "steps[].success_criteria must be an array");
            };
            let mut success_criteria = Vec::with_capacity(criteria_array.len());
            for item in criteria_array {
                let Some(s) = item.as_str() else {
                    return ai_error("INVALID_INPUT", "steps[].success_criteria items must be strings");
                };
                success_criteria.push(s.to_string());
            }
            steps.push(bm_storage::NewStep { title, success_criteria });
        }

        let result = self.store.steps_decompose(
            &workspace,
            &task_id,
            expected_revision,
            parent_path.as_ref(),
            steps,
        );

        match result {
            Ok(out) => ai_ok(
                "decompose",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "steps": out.steps.into_iter().map(|s| json!({ "step_id": s.step_id, "path": s.path })).collect::<Vec<_>>(),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Parent step not found"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_define(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if step_id.is_none() && path.is_none() {
            return ai_error("INVALID_INPUT", "step_id or path is required");
        }

        let title = match optional_non_null_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let success_criteria = match optional_string_array(args_obj, "success_criteria") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tests = match optional_string_array(args_obj, "tests") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let blockers = match optional_string_array(args_obj, "blockers") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = self.store.step_define(
            &workspace,
            &task_id,
            expected_revision,
            step_id.as_deref(),
            path.as_ref(),
            title,
            success_criteria,
            tests,
            blockers,
        );

        match result {
            Ok(out) => ai_ok(
                "define",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
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
            Err(StoreError::CheckpointsNotConfirmed { .. }) => ai_error("STORE_ERROR", "unexpected checkpoints error"),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_note(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let note = match require_string(args_obj, "note") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if step_id.is_none() && path.is_none() {
            return ai_error("INVALID_INPUT", "step_id or path is required");
        }

        let result = self.store.step_note(
            &workspace,
            &task_id,
            expected_revision,
            step_id.as_deref(),
            path.as_ref(),
            note,
        );

        match result {
            Ok(out) => ai_ok(
                "note",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
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
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_verify(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if step_id.is_none() && path.is_none() {
            return ai_error("INVALID_INPUT", "step_id or path is required");
        }

        let checkpoints = args_obj.get("checkpoints").cloned().unwrap_or(Value::Null);
        let criteria_confirmed = checkpoints
            .get("criteria")
            .and_then(|v| v.get("confirmed"))
            .and_then(|v| v.as_bool());
        let tests_confirmed = checkpoints
            .get("tests")
            .and_then(|v| v.get("confirmed"))
            .and_then(|v| v.as_bool());

        let result = self.store.step_verify(
            &workspace,
            &task_id,
            expected_revision,
            step_id.as_deref(),
            path.as_ref(),
            criteria_confirmed,
            tests_confirmed,
        );

        match result {
            Ok(out) => ai_ok(
                "verify",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
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
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_done(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if step_id.is_none() && path.is_none() {
            return ai_error("INVALID_INPUT", "step_id or path is required");
        }

        let result = self.store.step_done(
            &workspace,
            &task_id,
            expected_revision,
            step_id.as_deref(),
            path.as_ref(),
        );

        match result {
            Ok(out) => ai_ok(
                "done",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
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
            Err(StoreError::CheckpointsNotConfirmed { criteria, tests }) => ai_error_with(
                "CHECKPOINTS_NOT_CONFIRMED",
                &format!("missing: criteria={criteria} tests={tests}"),
                Some("Confirm missing checkpoints via tasks_verify before closing the step."),
                vec![suggest_call(
                    "tasks_verify",
                    "Confirm required checkpoints for this step.",
                    "high",
                    json!({
                        "workspace": workspace.as_str(),
                        "task": task_id,
                        "step_id": step_id,
                        "path": args_obj.get("path").cloned().unwrap_or(Value::Null),
                        "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } }
                    }),
                )],
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_edit(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let kind = if task_id.starts_with("PLAN-") {
            TaskKind::Plan
        } else if task_id.starts_with("TASK-") {
            TaskKind::Task
        } else {
            return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-");
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

        let contract = match optional_nullable_string(args_obj, "contract") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let contract_json = match optional_nullable_object_as_json_string(args_obj, "contract_data") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        match kind {
            TaskKind::Plan => {
                if description.is_some() {
                    return ai_error("INVALID_INPUT", "description is not valid for kind=plan");
                }
                if title.is_none() && contract.is_none() && contract_json.is_none() {
                    return ai_error("INVALID_INPUT", "no fields to edit");
                }
            }
            TaskKind::Task => {
                if contract.is_some() || contract_json.is_some() {
                    return ai_error("INVALID_INPUT", "contract fields are not valid for kind=task");
                }
                if title.is_none() && description.is_none() {
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
                &task_id,
                expected_revision,
                title,
                contract,
                contract_json,
                event_type.clone(),
                event_payload_json,
            ),
            TaskKind::Task => self.store.edit_task(
                &workspace,
                &task_id,
                expected_revision,
                title,
                description,
                event_type.clone(),
                event_payload_json,
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
            Err(StoreError::RevisionMismatch { expected, actual }) => {
                ai_error_with(
                    "REVISION_MISMATCH",
                    &format!("expected={expected} actual={actual}"),
                    Some("Refresh the current revision and retry with expected_revision."),
                    vec![suggest_call(
                        "tasks_context",
                        "Refresh current revisions for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                )
            }
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_context(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let plans = match self.store.list_plans(&workspace, 50, 0) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tasks = match self.store.list_tasks(&workspace, 50, 0) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "context",
            json!({
                "workspace": workspace.as_str(),
                "plans": plans.into_iter().map(|p| json!({
                    "id": p.id,
                    "revision": p.revision,
                    "title": p.title,
                    "contract": p.contract,
                    "contract_data": parse_json_or_null(p.contract_json),
                    "created_at_ms": p.created_at_ms,
                    "updated_at_ms": p.updated_at_ms
                })).collect::<Vec<_>>(),
                "tasks": tasks.into_iter().map(|t| json!({
                    "id": t.id,
                    "revision": t.revision,
                    "parent": t.parent_plan_id,
                    "title": t.title,
                    "description": t.description,
                    "created_at_ms": t.created_at_ms,
                    "updated_at_ms": t.updated_at_ms
                })).collect::<Vec<_>>()
            }),
        )
    }

    fn tool_tasks_delta(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let since = args_obj.get("since").and_then(|v| v.as_str());
        let limit = args_obj
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);

        let events = match self.store.list_events(&workspace, since, limit) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "delta",
            json!({
                "workspace": workspace.as_str(),
                "events": events.into_iter().map(|e| json!({
                    "event_id": e.event_id(),
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "task": e.task_id,
                    "path": e.path,
                    "type": e.event_type,
                    "payload": parse_json_or_string(&e.payload_json),
                })).collect::<Vec<_>>()
            }),
        )
    }

    fn tool_tasks_focus_get(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        match self.store.focus_get(&workspace) {
            Ok(focus) => ai_ok(
                "focus_get",
                json!({
                    "workspace": workspace.as_str(),
                    "focus": focus
                }),
            ),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_tasks_focus_set(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !task_id.starts_with("PLAN-") && !task_id.starts_with("TASK-") {
            return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-");
        }

        let prev = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        if let Err(err) = self.store.focus_set(&workspace, &task_id) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        ai_ok(
            "focus_set",
            json!({
                "workspace": workspace.as_str(),
                "previous": prev,
                "focus": task_id
            }),
        )
    }

    fn tool_tasks_focus_clear(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let prev = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let cleared = match self.store.focus_clear(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "focus_clear",
            json!({
                "workspace": workspace.as_str(),
                "previous": prev,
                "cleared": cleared
            }),
        )
    }

    fn tool_tasks_radar(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let requested_task = args_obj.get("task").and_then(|v| v.as_str()).map(|s| s.to_string());
        let focus = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let target_id = requested_task.or(focus.clone());
        let Some(target_id) = target_id else {
            return ai_error_with(
                "INVALID_INPUT",
                "No target: provide task or set focus",
                Some("Call tasks_context to list ids, then set focus with tasks_focus_set."),
                vec![suggest_call(
                    "tasks_context",
                    "List plans and tasks for this workspace to choose a focus target.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        };

        let kind = if target_id.starts_with("PLAN-") {
            TaskKind::Plan
        } else if target_id.starts_with("TASK-") {
            TaskKind::Task
        } else {
            return ai_error("INVALID_INPUT", "task must start with PLAN- or TASK-");
        };

        let target = match kind {
            TaskKind::Plan => match self.store.get_plan(&workspace, &target_id) {
                Ok(Some(p)) => json!({
                    "id": p.id,
                    "kind": "plan",
                    "revision": p.revision,
                    "title": p.title,
                    "contract": p.contract,
                    "contract_data": parse_json_or_null(p.contract_json),
                    "created_at_ms": p.created_at_ms,
                    "updated_at_ms": p.updated_at_ms
                }),
                Ok(None) => return ai_error("UNKNOWN_ID", "Unknown id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
            TaskKind::Task => match self.store.get_task(&workspace, &target_id) {
                Ok(Some(t)) => json!({
                    "id": t.id,
                    "kind": "task",
                    "revision": t.revision,
                    "parent": t.parent_plan_id,
                    "title": t.title,
                    "description": t.description,
                    "created_at_ms": t.created_at_ms,
                    "updated_at_ms": t.updated_at_ms
                }),
                Ok(None) => return ai_error("UNKNOWN_ID", "Unknown id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        };

        let reasoning_ref = match self.store.ensure_reasoning_ref(&workspace, &target_id, kind) {
            Ok(r) => json!({
                "branch": r.branch,
                "notes_doc": r.notes_doc,
                "graph_doc": r.graph_doc,
                "trace_doc": r.trace_doc
            }),
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let now = match kind {
            TaskKind::Plan => format!("Plan {}: {}", target_id, target.get("title").and_then(|v| v.as_str()).unwrap_or("")),
            TaskKind::Task => format!("Task {}: {}", target_id, target.get("title").and_then(|v| v.as_str()).unwrap_or("")),
        };

        let why = match kind {
            TaskKind::Plan => target
                .get("contract")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            TaskKind::Task => target
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        };

        let mut verify = Vec::<String>::new();
        let mut next = Vec::<String>::new();
        let mut blockers = Vec::<String>::new();
        let mut steps_summary: Option<Value> = None;

        if kind == TaskKind::Task {
            match self.store.task_steps_summary(&workspace, &target_id) {
                Ok(summary) => {
                    steps_summary = Some(json!({
                        "total": summary.total_steps,
                        "open": summary.open_steps,
                        "completed": summary.completed_steps,
                        "missing_criteria": summary.missing_criteria,
                        "missing_tests": summary.missing_tests,
                        "first_open": summary.first_open.as_ref().map(|s| json!({
                            "step_id": s.step_id,
                            "path": s.path,
                            "title": s.title,
                            "criteria_confirmed": s.criteria_confirmed,
                            "tests_confirmed": s.tests_confirmed
                        }))
                    }));

                    if summary.total_steps == 0 {
                        next.push("Add steps with tasks_decompose".to_string());
                    } else {
                        if summary.missing_criteria > 0 {
                            verify.push(format!("Missing criteria checkpoints: {}", summary.missing_criteria));
                        }
                        if summary.missing_tests > 0 {
                            verify.push(format!("Missing tests checkpoints: {}", summary.missing_tests));
                        }

                        if let Some(first) = summary.first_open {
                            if !first.criteria_confirmed || !first.tests_confirmed {
                                next.push(format!("Confirm checkpoints for {}", first.path));
                            } else {
                                next.push(format!("Close next step {}", first.path));
                            }
                        }
                    }
                }
                Err(StoreError::UnknownId) => {}
                Err(_) => {}
            }

            if let Ok(items) = self.store.task_open_blockers(&workspace, &target_id, 10) {
                blockers = items;
            }
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "requested": { "task": args_obj.get("task").cloned().unwrap_or(Value::Null) },
            "focus": focus,
            "target": target,
            "reasoning_ref": reasoning_ref,
            "radar": {
                "now": now,
                "why": why,
                "verify": verify,
                "next": next,
                "blockers": blockers
            }
        });
        if let Some(steps) = steps_summary {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("steps".to_string(), steps);
            }
        }

        if let Some(limit) = max_chars {
            let (used, truncated) = enforce_max_chars_budget(&mut result, limit);
            attach_budget(&mut result, limit, used, truncated);
        }

        ai_ok("radar", result)
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "storage",
            "description": "Get storage paths and namespaces.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "tasks_create",
            "description": "Create a plan or a task (v0 skeleton).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "kind": { "type": "string", "enum": ["plan", "task"] },
                    "parent": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "contract": { "type": "string" },
                    "contract_data": { "type": "object" },
                    "steps": { "type": "array" }
                },
                "required": ["workspace", "title"]
            }
        }),
        json!({
            "name": "tasks_decompose",
            "description": "Add steps to a task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "parent": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "success_criteria": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "success_criteria"]
                        }
                    }
                },
                "required": ["workspace", "task", "steps"]
            }
        }),
        json!({
            "name": "tasks_define",
            "description": "Update step fields (title/success_criteria/tests/blockers).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "title": { "type": "string" },
                    "success_criteria": { "type": "array", "items": { "type": "string" } },
                    "tests": { "type": "array", "items": { "type": "string" } },
                    "blockers": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "task"]
            }
        }),
        json!({
            "name": "tasks_note",
            "description": "Add a progress note to a step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["workspace", "task", "note"]
            }
        }),
        json!({
            "name": "tasks_verify",
            "description": "Confirm checkpoints for a step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "checkpoints": {
                        "type": "object",
                        "properties": {
                            "criteria": { "type": "object", "properties": { "confirmed": { "type": "boolean" } } },
                            "tests": { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }
                        }
                    }
                },
                "required": ["workspace", "task", "checkpoints"]
            }
        }),
        json!({
            "name": "tasks_done",
            "description": "Mark a step completed (requires confirmed checkpoints).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" }
                },
                "required": ["workspace", "task"]
            }
        }),
        json!({
            "name": "tasks_edit",
            "description": "Edit plan/task meta fields (v0).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "contract": { "type": "string" },
                    "contract_data": { "type": "object" }
                },
                "required": ["workspace", "task"]
            }
        }),
        json!({
            "name": "tasks_context",
            "description": "List plans and tasks in a workspace (v0 skeleton).",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": { "type": "string" } },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_delta",
            "description": "List events since an event id (v0 skeleton).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "since": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_focus_get",
            "description": "Get current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": { "type": "string" } },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_focus_set",
            "description": "Set current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" }
                },
                "required": ["workspace", "task"]
            }
        }),
        json!({
            "name": "tasks_focus_clear",
            "description": "Clear current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": { "type": "string" } },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_radar",
            "description": "Radar View: compact snapshot (Now/Why/Verify/Next/Blockers).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}

fn parse_kind(kind: Option<&str>, has_parent: bool) -> TaskKind {
    match kind {
        Some("task") => TaskKind::Task,
        Some("plan") => TaskKind::Plan,
        _ => {
            if has_parent {
                TaskKind::Task
            } else {
                TaskKind::Plan
            }
        }
    }
}

fn require_workspace(args: &serde_json::Map<String, Value>) -> Result<WorkspaceId, Value> {
    let Some(v) = args.get("workspace").and_then(|v| v.as_str()) else {
        return Err(ai_error("INVALID_INPUT", "workspace is required"));
    };
    match WorkspaceId::try_new(v.to_string()) {
        Ok(w) => Ok(w),
        Err(_) => Err(ai_error("INVALID_INPUT", "workspace is invalid")),
    }
}

fn require_string(args: &serde_json::Map<String, Value>, key: &str) -> Result<String, Value> {
    let Some(v) = args.get(key).and_then(|v| v.as_str()) else {
        return Err(ai_error("INVALID_INPUT", &format!("{key} is required")));
    };
    Ok(v.to_string())
}

fn optional_i64(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<i64>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{key} must be an integer"))),
        _ => Err(ai_error("INVALID_INPUT", &format!("{key} must be an integer"))),
    }
}

fn optional_string(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(v) => Ok(Some(v.to_string())),
        _ => Err(ai_error("INVALID_INPUT", &format!("{key} must be a string"))),
    }
}

fn optional_usize(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<usize>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_u64()
            .map(|v| v as usize)
            .map(Some)
            .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{key} must be a positive integer"))),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a positive integer"),
        )),
    }
}

fn optional_step_path(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<StepPath>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(raw) = value.as_str() else {
        return Err(ai_error("INVALID_INPUT", &format!("{key} must be a string")));
    };
    StepPath::parse(raw).map(Some).map_err(|_| ai_error("INVALID_INPUT", &format!("{key} is invalid")))
}

fn optional_string_array(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<Vec<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(arr) = value.as_array() else {
        return Err(ai_error("INVALID_INPUT", &format!("{key} must be an array of strings")));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err(ai_error("INVALID_INPUT", &format!("{key} must be an array of strings")));
        };
        out.push(s.to_string());
    }
    Ok(Some(out))
}

fn optional_non_null_string(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::String(v)) => Ok(Some(v.to_string())),
        Some(Value::Null) => Err(ai_error("INVALID_INPUT", &format!("{key} cannot be null"))),
        Some(_) => Err(ai_error("INVALID_INPUT", &format!("{key} must be a string"))),
        None => Ok(None),
    }
}

fn optional_nullable_string(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<Option<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(v)) => Ok(Some(Some(v.to_string()))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string or null"),
        )),
        None => Ok(None),
    }
}

fn optional_nullable_object_as_json_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Object(_)) => Ok(Some(Some(
            args.get(key)
                .expect("key exists")
                .to_string(),
        ))),
        Some(_) => Err(ai_error("INVALID_INPUT", &format!("{key} must be an object or null"))),
        None => Ok(None),
    }
}

fn parse_json_or_null(value: Option<String>) -> Value {
    match value {
        None => Value::Null,
        Some(raw) => serde_json::from_str(&raw).unwrap_or(Value::Null),
    }
}

fn parse_json_or_string(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::RevisionMismatch { expected, actual } => format!("Revision mismatch: expected={expected} actual={actual}"),
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::StepNotFound => "Step not found".to_string(),
        StoreError::CheckpointsNotConfirmed { criteria, tests } => {
            format!("Checkpoints not confirmed: criteria={criteria} tests={tests}")
        }
    }
}

fn suggest_call(target: &str, reason: &str, priority: &str, params: Value) -> Value {
    json!({
        "action": "call_tool",
        "target": target,
        "reason": reason,
        "priority": priority,
        "validated": true,
        "params": params
    })
}

fn ai_ok_with(intent: &str, result: Value, suggestions: Vec<Value>) -> Value {
    json!({
        "success": true,
        "intent": intent,
        "result": result,
        "warnings": [],
        "suggestions": suggestions,
        "context": {},
        "error": null,
        "timestamp": now_rfc3339(),
    })
}

fn ai_error_with(code: &str, message: &str, recovery: Option<&str>, suggestions: Vec<Value>) -> Value {
    let error = match recovery {
        None => json!({ "code": code, "message": message }),
        Some(recovery) => json!({ "code": code, "message": message, "recovery": recovery }),
    };
    json!({
        "success": false,
        "intent": "error",
        "result": {},
        "warnings": [],
        "suggestions": suggestions,
        "context": {},
        "error": error,
        "timestamp": now_rfc3339(),
    })
}

fn ai_ok(intent: &str, result: Value) -> Value {
    ai_ok_with(intent, result, Vec::new())
}

fn ai_error(code: &str, message: &str) -> Value {
    ai_error_with(code, message, None, Vec::new())
}

fn now_rfc3339() -> Value {
    Value::String(
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
    )
}

fn ts_ms_to_rfc3339(ts_ms: i64) -> String {
    let nanos = (ts_ms as i128) * 1_000_000i128;
    let dt = OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    dt.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn json_len_chars(value: &Value) -> usize {
    serde_json::to_string(value).map(|s| s.len()).unwrap_or(0)
}

fn truncate_string(value: &str, max_chars: usize) -> String {
    if value.len() <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn enforce_max_chars_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if let Some(why) = value
        .get_mut("radar")
        .and_then(|v| v.get_mut("why"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        let shorter = truncate_string(&why, 256);
        if let Some(radar) = value.get_mut("radar") {
            if let Some(obj) = radar.as_object_mut() {
                obj.insert("why".to_string(), Value::String(shorter));
            }
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    if let Some(target) = value.get_mut("target").and_then(|v| v.as_object_mut()) {
        target.remove("contract_data");
        target.remove("contract");
        target.remove("description");
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    (used, truncated)
}

fn attach_budget(value: &mut Value, max_chars: usize, used_chars: usize, truncated: bool) {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "budget".to_string(),
            json!({
                "max_chars": max_chars,
                "used_chars": used_chars,
                "truncated": truncated
            }),
        );
    }
}

fn parse_storage_dir() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let mut storage_dir: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--storage-dir" => {
                if let Some(value) = args.next() {
                    storage_dir = Some(PathBuf::from(value));
                }
            }
            _ => {}
        }
    }
    storage_dir.unwrap_or_else(|| PathBuf::from(".branchmind_rust"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage_dir = parse_storage_dir();
    let store = SqliteStore::open(storage_dir)?;
    let mut server = McpServer::new(store);

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(v) => v,
            Err(_) => break,
        };
        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }

        let parsed: Result<Value, _> = serde_json::from_str(raw);
        let data = match parsed {
            Ok(v) => v,
            Err(e) => {
                let resp = json_rpc_error(None, -32700, &format!("Parse error: {e}"));
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                stdout.flush()?;
                continue;
            }
        };
        let (id, has_method) = match data.as_object() {
            Some(obj) => (obj.get("id").cloned(), obj.contains_key("method")),
            None => {
                let resp = json_rpc_error(None, -32600, "Invalid Request");
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                stdout.flush()?;
                continue;
            }
        };
        if !has_method {
            let resp = json_rpc_error(id, -32600, "Invalid Request");
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_value(data) {
            Ok(v) => v,
            Err(e) => {
                let resp = json_rpc_error(id, -32600, &format!("Invalid Request: {e}"));
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                stdout.flush()?;
                continue;
            }
        };

        if let Some(resp) = server.handle(request) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}
