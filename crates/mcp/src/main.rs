#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use bm_storage::{SqliteStore, StoreError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MCP_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "branchmind-rust-mcp";
const SERVER_VERSION: &str = "0.1.0";
const DEFAULT_NOTES_DOC: &str = "notes";
const DEFAULT_GRAPH_DOC: &str = "graph";
const DEFAULT_TRACE_DOC: &str = "trace";

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
                Value::String(
                    serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string()),
                ),
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
            return Some(json_rpc_response(
                request.id,
                json!({ "tools": tool_definitions() }),
            ));
        }

        if method == "tools/call" {
            let Some(params) = request.params else {
                return Some(json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };
            let Some(params_obj) = params.as_object() else {
                return Some(json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };

            let tool_name = params_obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params_obj
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response_body = self.call_tool(tool_name, args);

            return Some(json_rpc_response(
                request.id,
                json!({
                    "content": [tool_text_content(&response_body)],
                    "isError": !response_body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                }),
            ));
        }

        Some(json_rpc_error(
            request.id,
            -32601,
            &format!("Method not found: {method}"),
        ))
    }

    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        match name {
            "tasks_create" => self.tool_tasks_create(args),
            "tasks_decompose" => self.tool_tasks_decompose(args),
            "tasks_define" => self.tool_tasks_define(args),
            "tasks_note" => self.tool_tasks_note(args),
            "tasks_verify" => self.tool_tasks_verify(args),
            "tasks_done" => self.tool_tasks_done(args),
            "tasks_close_step" => self.tool_tasks_close_step(args),
            "tasks_edit" => self.tool_tasks_edit(args),
            "tasks_context" => self.tool_tasks_context(args),
            "tasks_delta" => self.tool_tasks_delta(args),
            "tasks_focus_get" => self.tool_tasks_focus_get(args),
            "tasks_focus_set" => self.tool_tasks_focus_set(args),
            "tasks_focus_clear" => self.tool_tasks_focus_clear(args),
            "tasks_radar" => self.tool_tasks_radar(args),
            "branchmind_init" => self.tool_branchmind_init(args),
            "branchmind_status" => self.tool_branchmind_status(args),
            "branchmind_branch_create" => self.tool_branchmind_branch_create(args),
            "branchmind_branch_list" => self.tool_branchmind_branch_list(args),
            "branchmind_checkout" => self.tool_branchmind_checkout(args),
            "branchmind_notes_commit" => self.tool_branchmind_notes_commit(args),
            "branchmind_show" => self.tool_branchmind_show(args),
            "branchmind_diff" => self.tool_branchmind_diff(args),
            "branchmind_merge" => self.tool_branchmind_merge(args),
            "branchmind_graph_apply" => self.tool_branchmind_graph_apply(args),
            "branchmind_graph_query" => self.tool_branchmind_graph_query(args),
            "branchmind_graph_validate" => self.tool_branchmind_graph_validate(args),
            "branchmind_graph_diff" => self.tool_branchmind_graph_diff(args),
            "branchmind_graph_merge" => self.tool_branchmind_graph_merge(args),
            "branchmind_graph_conflicts" => self.tool_branchmind_graph_conflicts(args),
            "branchmind_graph_conflict_show" => self.tool_branchmind_graph_conflict_show(args),
            "branchmind_graph_conflict_resolve" => {
                self.tool_branchmind_graph_conflict_resolve(args)
            }
            "branchmind_think_template" => self.tool_branchmind_think_template(args),
            "branchmind_think_card" => self.tool_branchmind_think_card(args),
            "branchmind_think_context" => self.tool_branchmind_think_context(args),
            "branchmind_export" => self.tool_branchmind_export(args),
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

        let parent = args_obj
            .get("parent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let kind = parse_kind(
            args_obj.get("kind").and_then(|v| v.as_str()),
            parent.is_some(),
        );

        let description = args_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let contract = args_obj
            .get("contract")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let contract_json = args_obj.get("contract_data").map(|v| v.to_string());

        if args_obj.get("steps").is_some() {
            return ai_error(
                "NOT_IMPLEMENTED",
                "steps are not implemented in v0 skeleton",
            );
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
                    return ai_error(
                        "INVALID_INPUT",
                        "steps[].success_criteria items must be strings",
                    );
                };
                success_criteria.push(s.to_string());
            }
            steps.push(bm_storage::NewStep {
                title,
                success_criteria,
            });
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
            Err(StoreError::CheckpointsNotConfirmed { .. }) => {
                ai_error("STORE_ERROR", "unexpected checkpoints error")
            }
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

    fn tool_tasks_close_step(&mut self, args: Value) -> Value {
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

        let checkpoints = match args_obj.get("checkpoints") {
            Some(v) => v.clone(),
            None => return ai_error("INVALID_INPUT", "checkpoints is required"),
        };
        let criteria_confirmed = checkpoints
            .get("criteria")
            .and_then(|v| v.get("confirmed"))
            .and_then(|v| v.as_bool());
        let tests_confirmed = checkpoints
            .get("tests")
            .and_then(|v| v.get("confirmed"))
            .and_then(|v| v.as_bool());

        let result = self.store.step_close(
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
                "close_step",
                json!({
                    "task": task_id,
                    "revision": out.task_revision,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "events": out.events.into_iter().map(|event| json!({
                        "event_id": event.event_id(),
                        "ts": ts_ms_to_rfc3339(event.ts_ms),
                        "ts_ms": event.ts_ms,
                        "task_id": event.task_id,
                        "path": event.path,
                        "type": event.event_type,
                        "payload": parse_json_or_string(&event.payload_json)
                    })).collect::<Vec<_>>()
                }),
            ),
            Err(StoreError::CheckpointsNotConfirmed { criteria, tests }) => ai_error_with(
                "CHECKPOINTS_NOT_CONFIRMED",
                &format!("missing: criteria={criteria} tests={tests}"),
                Some("Confirm missing checkpoints before closing the step."),
                vec![suggest_call(
                    "tasks_close_step",
                    "Confirm required checkpoints and close the step.",
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

        let contract_json = match optional_nullable_object_as_json_string(args_obj, "contract_data")
        {
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
                    return ai_error(
                        "INVALID_INPUT",
                        "contract fields are not valid for kind=task",
                    );
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

    fn tool_tasks_context(&mut self, args: Value) -> Value {
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

        let plans = match self.store.list_plans(&workspace, 50, 0) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tasks = match self.store.list_tasks(&workspace, 50, 0) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
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
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let mut truncated = false;
            let (_used, tasks_truncated) = enforce_graph_list_budget(&mut result, "tasks", limit);
            truncated |= tasks_truncated;
            let (_used, plans_truncated) = enforce_graph_list_budget(&mut result, "plans", limit);
            truncated |= plans_truncated;
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used, tasks_truncated) =
                    enforce_graph_list_budget(&mut result, "tasks", limit);
                let (_used, plans_truncated) =
                    enforce_graph_list_budget(&mut result, "plans", limit);
                let _ = attach_budget(
                    &mut result,
                    limit,
                    truncated || tasks_truncated || plans_truncated,
                );
            }
        }

        ai_ok("context", result)
    }

    fn tool_tasks_delta(&mut self, args: Value) -> Value {
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

        let mut result = json!({
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
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "events", limit);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used, truncated2) = enforce_graph_list_budget(&mut result, "events", limit);
                let _ = attach_budget(&mut result, limit, truncated || truncated2);
            }
        }

        ai_ok("delta", result)
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

        let requested_task = args_obj
            .get("task")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
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

        let reasoning_ref = match self
            .store
            .ensure_reasoning_ref(&workspace, &target_id, kind)
        {
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
            TaskKind::Plan => format!(
                "Plan {}: {}",
                target_id,
                target.get("title").and_then(|v| v.as_str()).unwrap_or("")
            ),
            TaskKind::Task => format!(
                "Task {}: {}",
                target_id,
                target.get("title").and_then(|v| v.as_str()).unwrap_or("")
            ),
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
                            verify.push(format!(
                                "Missing criteria checkpoints: {}",
                                summary.missing_criteria
                            ));
                        }
                        if summary.missing_tests > 0 {
                            verify.push(format!(
                                "Missing tests checkpoints: {}",
                                summary.missing_tests
                            ));
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
            let (_used, truncated) = enforce_max_chars_budget(&mut result, limit);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_max_chars_budget(&mut result, limit);
                let _ = attach_budget(&mut result, limit, truncated || truncated2);
            }
        }

        ai_ok("radar", result)
    }

    fn tool_branchmind_init(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        match self.store.workspace_init(&workspace) {
            Ok(()) => {
                let checkout = self
                    .store
                    .branch_checkout_get(&workspace)
                    .ok()
                    .flatten();
                let mut suggestions = Vec::new();
                if checkout.is_some() {
                    suggestions.push(suggest_call(
                        "branchmind_think_card",
                        "Start with a lightweight note.",
                        "high",
                        json!({ "workspace": workspace.as_str(), "card": "First note" }),
                    ));
                }
                ai_ok_with(
                    "branchmind_init",
                    json!({
                        "workspace": workspace.as_str(),
                        "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
                        "schema_version": "v0"
                    }),
                    suggestions,
                )
            }
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    fn tool_branchmind_status(&mut self, args: Value) -> Value {
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

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let last_event = match self.store.workspace_last_event_head(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let last_doc_entry = match self.store.workspace_last_doc_entry_head(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let checkout = match self.store.branch_checkout_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "schema_version": "v0",
            "workspace_exists": workspace_exists,
            "last_event": last_event.map(|(seq, ts_ms)| json!({
                "event_id": format!("evt_{:016}", seq),
                "ts": ts_ms_to_rfc3339(ts_ms),
                "ts_ms": ts_ms
            })),
            "last_doc_entry": last_doc_entry.map(|(seq, ts_ms, branch, doc, kind)| json!({
                "seq": seq,
                "ts": ts_ms_to_rfc3339(ts_ms),
                "ts_ms": ts_ms,
                "branch": branch,
                "doc": doc,
                "kind": kind
            })),
        });

        let mut suggestions = Vec::new();
        if !workspace_exists {
            suggestions.push(suggest_call(
                "branchmind_init",
                "Initialize the workspace and bootstrap a default branch.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            ));
        } else if checkout.is_none() {
            suggestions.push(suggest_call(
                "branchmind_branch_list",
                "List known branches for this workspace.",
                "medium",
                json!({ "workspace": workspace.as_str() }),
            ));
        }

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_show_budget(&mut result, limit);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_branchmind_show_budget(&mut result, limit);
                let _ = attach_budget(&mut result, limit, truncated || truncated2);
            }
        }

        ai_ok_with("branchmind_status", result, suggestions)
    }

    fn tool_branchmind_branch_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let name = match require_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from = match optional_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let info = match self.store.branch_create(&workspace, &name, from.as_deref()) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown base branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::BranchAlreadyExists) => {
                return ai_error_with(
                    "CONFLICT",
                    "Branch already exists",
                    Some("Choose a different name (or delete/rename the existing branch)."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::BranchCycle) => return ai_error("INVALID_INPUT", "Branch base cycle"),
            Err(StoreError::BranchDepthExceeded) => {
                return ai_error("INVALID_INPUT", "Branch base depth exceeded");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_branch_create",
            json!({
                "workspace": workspace.as_str(),
                "branch": {
                    "name": info.name,
                    "base_branch": info.base_branch,
                    "base_seq": info.base_seq
                }
            }),
        )
    }

    fn tool_branchmind_branch_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let branches = match self.store.branch_list(&workspace, limit) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let branches_json = branches
            .into_iter()
            .map(|b| {
                json!({
                    "name": b.name,
                    "base_branch": b.base_branch,
                    "base_seq": b.base_seq,
                    "created_at_ms": b.created_at_ms
                })
            })
            .collect::<Vec<_>>();

        let count = branches_json.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branches": branches_json,
            "count": count,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_branch_list_budget(&mut result, limit);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) =
                    enforce_branchmind_branch_list_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_branch_list", result)
    }

    fn tool_branchmind_checkout(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let reference = match require_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (previous, current) = match self.store.branch_checkout_set(&workspace, &reference) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_checkout",
            json!({
                "workspace": workspace.as_str(),
                "previous": previous,
                "current": current
            }),
        )
    }

    fn tool_branchmind_notes_commit(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let content = match require_string(args_obj, "content") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if content.trim().is_empty() {
            return ai_error("INVALID_INPUT", "content must not be empty");
        }

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (reasoning.branch, reasoning.notes_doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| DEFAULT_NOTES_DOC.to_string());
                (branch, doc)
            }
        };

        let title = match optional_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let format = match optional_string(args_obj, "format") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_json = match optional_object_as_json_string(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let entry = match self
            .store
            .doc_append_note(&workspace, &branch, &doc, title, format, meta_json, content)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "entry": {
                "seq": entry.seq,
                "ts": ts_ms_to_rfc3339(entry.ts_ms),
                "ts_ms": entry.ts_ms,
                "branch": entry.branch,
                "doc": entry.doc,
                "kind": entry.kind.as_str(),
                "title": entry.title,
                "format": entry.format,
                "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                "content": entry.content
            }
        });
        redact_value(&mut result, 6);
        ai_ok("branchmind_notes_commit", result)
    }

    fn tool_branchmind_show(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let doc_kind = args_obj
            .get("doc_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("trace");
        if doc_kind != "trace" && doc_kind != "notes" {
            return ai_error("INVALID_INPUT", "doc_kind must be 'trace' or 'notes'");
        }

        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let doc = match doc_kind {
                    "trace" => reasoning.trace_doc,
                    "notes" => reasoning.notes_doc,
                    _ => return ai_error("INVALID_INPUT", "doc_kind must be 'trace' or 'notes'"),
                };
                (reasoning.branch, doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| match doc_kind {
                    "notes" => DEFAULT_NOTES_DOC.to_string(),
                    _ => DEFAULT_TRACE_DOC.to_string(),
                });
                (branch, doc)
            }
        };

        let slice = match self
            .store
            .doc_show_tail(&workspace, &branch, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entries = slice
            .entries
            .into_iter()
            .map(|e| match e.kind {
                bm_storage::DocEntryKind::Note => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "title": e.title,
                    "format": e.format,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "content": e.content
                }),
                bm_storage::DocEntryKind::Event => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "event_id": e.source_event_id,
                    "event_type": e.event_type,
                    "task_id": e.task_id,
                    "path": e.path
                }),
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "entries": entries,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": entries.len()
            },
            "truncated": false
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_show_budget(&mut result, limit);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_branchmind_show_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_show", result)
    }

    fn tool_branchmind_diff(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| "notes".to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let to_exists = match self.store.branch_exists(&workspace, &to) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !to_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown to-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let slice = match self
            .store
            .doc_diff_tail(&workspace, &from, &to, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entries = slice
            .entries
            .into_iter()
            .map(|e| match e.kind {
                bm_storage::DocEntryKind::Note => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "title": e.title,
                    "format": e.format,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "content": e.content
                }),
                bm_storage::DocEntryKind::Event => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "event_id": e.source_event_id,
                    "event_type": e.event_type,
                    "task_id": e.task_id,
                    "path": e.path
                }),
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "from": from,
            "to": to,
            "doc": doc,
            "entries": entries,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": entries.len()
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_show_budget(&mut result, limit);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_branchmind_show_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_diff", result)
    }

    fn tool_branchmind_merge(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let into = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| "notes".to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let into_exists = match self.store.branch_exists(&workspace, &into) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !into_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown into-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let merged = match self
            .store
            .doc_merge_notes(&workspace, &from, &into, &doc, cursor, limit, dry_run)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_merge",
            json!({
                "workspace": workspace.as_str(),
                "from": from,
                "into": into,
                "doc": doc,
                "merged": merged.merged,
                "skipped": merged.skipped,
                "pagination": {
                    "cursor": cursor,
                    "next_cursor": merged.next_cursor,
                    "has_more": merged.has_more,
                    "limit": limit,
                    "count": merged.count
                }
            }),
        )
    }

    fn tool_branchmind_graph_apply(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (reasoning.branch, reasoning.graph_doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                (branch, doc)
            }
        };

        let ops_value = args_obj.get("ops").cloned().unwrap_or(Value::Null);
        let Some(ops_array) = ops_value.as_array() else {
            return ai_error("INVALID_INPUT", "ops must be an array");
        };
        if ops_array.is_empty() {
            return ai_error("INVALID_INPUT", "ops must not be empty");
        }
        let mut ops = Vec::with_capacity(ops_array.len());
        for op_value in ops_array {
            let Some(op_obj) = op_value.as_object() else {
                return ai_error("INVALID_INPUT", "ops[] must be an array of objects");
            };
            let op_name = op_obj.get("op").and_then(|v| v.as_str()).unwrap_or("");
            match op_name {
                "node_upsert" => {
                    let id = match require_string(op_obj, "id") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let node_type = match require_string(op_obj, "type") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let title = match optional_string(op_obj, "title") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let text = match optional_string(op_obj, "text") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let status = match optional_string(op_obj, "status") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let tags = match optional_string_array(op_obj, "tags") {
                        Ok(v) => v.unwrap_or_default(),
                        Err(resp) => return resp,
                    };
                    let meta_json = match optional_object_as_json_string(op_obj, "meta") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::NodeUpsert(
                        bm_storage::GraphNodeUpsert {
                            id,
                            node_type,
                            title,
                            text,
                            tags,
                            status,
                            meta_json,
                        },
                    ));
                }
                "node_delete" => {
                    let id = match require_string(op_obj, "id") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::NodeDelete { id });
                }
                "edge_upsert" => {
                    let from = match require_string(op_obj, "from") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let rel = match require_string(op_obj, "rel") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let to = match require_string(op_obj, "to") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let meta_json = match optional_object_as_json_string(op_obj, "meta") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::EdgeUpsert(
                        bm_storage::GraphEdgeUpsert {
                            from,
                            rel,
                            to,
                            meta_json,
                        },
                    ));
                }
                "edge_delete" => {
                    let from = match require_string(op_obj, "from") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let rel = match require_string(op_obj, "rel") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    let to = match require_string(op_obj, "to") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    };
                    ops.push(bm_storage::GraphOp::EdgeDelete { from, rel, to });
                }
                _ => {
                    return ai_error(
                        "INVALID_INPUT",
                        "ops[].op must be one of: node_upsert|node_delete|edge_upsert|edge_delete",
                    );
                }
            }
        }

        let applied = match self.store.graph_apply_ops(&workspace, &branch, &doc, ops) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_graph_apply",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "doc": doc,
                "applied": {
                    "nodes_upserted": applied.nodes_upserted,
                    "nodes_deleted": applied.nodes_deleted,
                    "edges_upserted": applied.edges_upserted,
                    "edges_deleted": applied.edges_deleted
                },
                "last_seq": applied.last_seq,
                "last_ts_ms": applied.last_ts_ms
            }),
        )
    }

    fn tool_branchmind_graph_query(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (reasoning.branch, reasoning.graph_doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                (branch, doc)
            }
        };

        let ids = match optional_string_array(args_obj, "ids") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let types = match optional_string_array(args_obj, "types") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_any = match optional_string_array(args_obj, "tags_any") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let tags_all = match optional_string_array(args_obj, "tags_all") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let text = match optional_string(args_obj, "text") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let include_edges = match optional_bool(args_obj, "include_edges") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let edges_limit = match optional_usize(args_obj, "edges_limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let request = bm_storage::GraphQueryRequest {
            ids,
            types,
            status,
            tags_any,
            tags_all,
            text,
            cursor,
            limit,
            include_edges,
            edges_limit,
        };

        let slice = match self.store.graph_query(&workspace, &branch, &doc, request) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let nodes = slice
            .nodes
            .into_iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "type": n.node_type,
                    "title": n.title,
                    "text": n.text,
                    "status": n.status,
                    "tags": n.tags,
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            })
            .collect::<Vec<_>>();
        let edges = slice
            .edges
            .into_iter()
            .map(|e| {
                json!({
                    "from": e.from,
                    "rel": e.rel,
                    "to": e.to,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": e.deleted,
                    "last_seq": e.last_seq,
                    "last_ts_ms": e.last_ts_ms
                })
            })
            .collect::<Vec<_>>();

        let node_count = nodes.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "nodes": nodes,
            "edges": edges,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": node_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before_nodes = result
                .get("nodes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_query_budget(&mut result, limit);
            let after_nodes = result
                .get("nodes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            if after_nodes < before_nodes {
                if let Some(next_cursor) = result
                    .get("nodes")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("last_seq"))
                    .and_then(|v| v.as_i64())
                {
                    if let Some(pagination) =
                        result.get_mut("pagination").and_then(|v| v.as_object_mut())
                    {
                        pagination.insert(
                            "next_cursor".to_string(),
                            Value::Number(serde_json::Number::from(next_cursor)),
                        );
                        pagination.insert("has_more".to_string(), Value::Bool(true));
                        pagination.insert(
                            "count".to_string(),
                            Value::Number(serde_json::Number::from(after_nodes as u64)),
                        );
                    }
                }
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_query_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_graph_query", result)
    }

    fn tool_branchmind_graph_validate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (reasoning.branch, reasoning.graph_doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                (branch, doc)
            }
        };

        let max_errors = match optional_usize(args_obj, "max_errors") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let validation = match self
            .store
            .graph_validate(&workspace, &branch, &doc, max_errors)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let errors = validation
            .errors
            .into_iter()
            .map(|e| {
                json!({
                    "code": e.code,
                    "message": e.message,
                    "kind": e.kind,
                    "key": e.key
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "ok": validation.ok,
            "stats": { "nodes": validation.nodes, "edges": validation.edges },
            "errors": errors,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "errors", limit);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "errors", limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_graph_validate", result)
    }

    fn tool_branchmind_graph_diff(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let to_exists = match self.store.branch_exists(&workspace, &to) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !to_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown to-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let slice = match self
            .store
            .graph_diff(&workspace, &from, &to, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let changes = slice
            .changes
            .into_iter()
            .map(|c| match c {
                bm_storage::GraphDiffChange::Node { to: n } => {
                    let id = n.id.clone();
                    json!({
                        "kind": "node",
                        "id": id,
                        "to": {
                            "id": n.id,
                            "type": n.node_type,
                            "title": n.title,
                            "text": n.text,
                            "status": n.status,
                            "tags": n.tags,
                            "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                            "deleted": n.deleted,
                            "last_seq": n.last_seq,
                            "last_ts_ms": n.last_ts_ms
                        }
                    })
                }
                bm_storage::GraphDiffChange::Edge { to: e } => {
                    let from = e.from.clone();
                    let rel = e.rel.clone();
                    let to = e.to.clone();
                    json!({
                        "kind": "edge",
                        "key": { "from": from, "rel": rel, "to": to },
                        "to": {
                            "from": e.from,
                            "rel": e.rel,
                            "to": e.to,
                            "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                            "deleted": e.deleted,
                            "last_seq": e.last_seq,
                            "last_ts_ms": e.last_ts_ms
                        }
                    })
                }
            })
            .collect::<Vec<_>>();

        let change_count = changes.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "from": from,
            "to": to,
            "doc": doc,
            "changes": changes,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": change_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before = result
                .get("changes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "changes", limit);
            let after = result
                .get("changes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            if after < before {
                if let Some(next_cursor) = result
                    .get("changes")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("to"))
                    .and_then(|v| v.get("last_seq"))
                    .and_then(|v| v.as_i64())
                {
                    if let Some(pagination) =
                        result.get_mut("pagination").and_then(|v| v.as_object_mut())
                    {
                        pagination.insert(
                            "next_cursor".to_string(),
                            Value::Number(serde_json::Number::from(next_cursor)),
                        );
                        pagination.insert("has_more".to_string(), Value::Bool(true));
                        pagination.insert(
                            "count".to_string(),
                            Value::Number(serde_json::Number::from(after as u64)),
                        );
                    }
                }
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "changes", limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_graph_diff", result)
    }

    fn tool_branchmind_graph_merge(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let into = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let into_exists = match self.store.branch_exists(&workspace, &into) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !into_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown into-branch",
                Some("Call branchmind_branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let merged = match self
            .store
            .graph_merge_back(&workspace, &from, &into, &doc, cursor, limit, dry_run)
        {
            Ok(v) => v,
            Err(StoreError::MergeNotSupported) => {
                return ai_error_with(
                    "MERGE_NOT_SUPPORTED",
                    "Merge not supported",
                    Some("v0 supports only merge-back into base: from.base_branch == into"),
                    vec![],
                );
            }
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_graph_merge",
            json!({
                "workspace": workspace.as_str(),
                "from": from,
                "into": into,
                "doc": doc,
                "merged": merged.merged,
                "skipped": merged.skipped,
                "conflicts_created": merged.conflicts_created,
                "conflict_ids": merged.conflict_ids,
                "pagination": {
                    "cursor": cursor,
                    "next_cursor": merged.next_cursor,
                    "has_more": merged.has_more,
                    "limit": limit,
                    "count": merged.count
                }
            }),
        )
    }

    fn tool_branchmind_graph_conflicts(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let into = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (conflicts, next_cursor, has_more) = match self.store.graph_conflicts_list(
            &workspace,
            &into,
            &doc,
            status.as_deref(),
            cursor,
            limit,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let conflicts = conflicts
            .into_iter()
            .map(|c| {
                json!({
                    "conflict_id": c.conflict_id,
                    "kind": c.kind,
                    "key": c.key,
                    "status": c.status,
                    "created_at_ms": c.created_at_ms
                })
            })
            .collect::<Vec<_>>();

        let conflict_count = conflicts.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "into": into,
            "doc": doc,
            "conflicts": conflicts,
            "pagination": {
                "cursor": cursor,
                "next_cursor": next_cursor,
                "has_more": has_more,
                "limit": limit,
                "count": conflict_count
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before = result
                .get("conflicts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "conflicts", limit);
            let after = result
                .get("conflicts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            if after < before {
                if let Some(next_cursor) = result
                    .get("conflicts")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.get("created_at_ms"))
                    .and_then(|v| v.as_i64())
                {
                    if let Some(pagination) =
                        result.get_mut("pagination").and_then(|v| v.as_object_mut())
                    {
                        pagination.insert(
                            "next_cursor".to_string(),
                            Value::Number(serde_json::Number::from(next_cursor)),
                        );
                        pagination.insert("has_more".to_string(), Value::Bool(true));
                        pagination.insert(
                            "count".to_string(),
                            Value::Number(serde_json::Number::from(after as u64)),
                        );
                    }
                }
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) =
                    enforce_graph_list_budget(&mut result, "conflicts", limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_graph_conflicts", result)
    }

    fn tool_branchmind_graph_conflict_show(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let conflict_id = match require_string(args_obj, "conflict_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let detail = match self.store.graph_conflict_show(&workspace, &conflict_id) {
            Ok(v) => v,
            Err(StoreError::UnknownConflict) => return ai_error("UNKNOWN_ID", "Unknown conflict"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let base = if detail.kind == "node" {
            detail.base_node.as_ref().map(|n| {
                json!({
                    "id": n.id.clone(),
                    "type": n.node_type.clone(),
                    "title": n.title.clone(),
                    "text": n.text.clone(),
                    "status": n.status.clone(),
                    "tags": n.tags.clone(),
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        } else {
            detail.base_edge.as_ref().map(|e| {
                json!({
                    "from": e.from.clone(),
                    "rel": e.rel.clone(),
                    "to": e.to.clone(),
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": e.deleted,
                    "last_seq": e.last_seq,
                    "last_ts_ms": e.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        };
        let theirs = if detail.kind == "node" {
            detail.theirs_node.as_ref().map(|n| {
                json!({
                    "id": n.id.clone(),
                    "type": n.node_type.clone(),
                    "title": n.title.clone(),
                    "text": n.text.clone(),
                    "status": n.status.clone(),
                    "tags": n.tags.clone(),
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        } else {
            detail.theirs_edge.as_ref().map(|e| {
                json!({
                    "from": e.from.clone(),
                    "rel": e.rel.clone(),
                    "to": e.to.clone(),
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": e.deleted,
                    "last_seq": e.last_seq,
                    "last_ts_ms": e.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        };
        let ours = if detail.kind == "node" {
            detail.ours_node.as_ref().map(|n| {
                json!({
                    "id": n.id.clone(),
                    "type": n.node_type.clone(),
                    "title": n.title.clone(),
                    "text": n.text.clone(),
                    "status": n.status.clone(),
                    "tags": n.tags.clone(),
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        } else {
            detail.ours_edge.as_ref().map(|e| {
                json!({
                    "from": e.from.clone(),
                    "rel": e.rel.clone(),
                    "to": e.to.clone(),
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": e.deleted,
                    "last_seq": e.last_seq,
                    "last_ts_ms": e.last_ts_ms
                })
            }).unwrap_or(Value::Null)
        };

        ai_ok(
            "branchmind_graph_conflict_show",
            json!({
                "workspace": workspace.as_str(),
                "conflict": {
                    "conflict_id": detail.conflict_id,
                    "kind": detail.kind,
                    "key": detail.key,
                    "from": detail.from_branch,
                    "into": detail.into_branch,
                    "doc": detail.doc,
                    "status": detail.status,
                    "created_at_ms": detail.created_at_ms,
                    "resolved_at_ms": detail.resolved_at_ms,
                    "base": base,
                    "theirs": theirs,
                    "ours": ours
                }
            }),
        )
    }

    fn tool_branchmind_graph_conflict_resolve(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let conflict_id = match require_string(args_obj, "conflict_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let resolution = match require_string(args_obj, "resolution") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let resolved =
            match self
                .store
                .graph_conflict_resolve(&workspace, &conflict_id, &resolution)
            {
                Ok(v) => v,
                Err(StoreError::UnknownConflict) => {
                    return ai_error("UNKNOWN_ID", "Unknown conflict");
                }
                Err(StoreError::ConflictAlreadyResolved) => {
                    return ai_error("INVALID_INPUT", "Conflict already resolved");
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        ai_ok(
            "branchmind_graph_conflict_resolve",
            json!({
                "workspace": workspace.as_str(),
                "conflict_id": resolved.conflict_id,
                "status": resolved.status,
                "applied": resolved.applied
            }),
        )
    }

    fn tool_branchmind_think_template(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let card_type = match require_string(args_obj, "type") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let card_type = card_type.trim().to_string();
        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        if !bm_core::think::is_supported_think_card_type(&card_type) {
            return ai_error_with(
                "INVALID_INPUT",
                "Unsupported card type",
                Some(&format!(
                    "Supported: {}",
                    supported.iter().copied().collect::<Vec<_>>().join(", ")
                )),
                vec![suggest_call(
                    "branchmind_think_template",
                    "Request a supported template type.",
                    "high",
                    json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
                )],
            );
        }

        let template = json!({
            "id": "CARD-<id>",
            "type": card_type,
            "title": null,
            "text": null,
            "status": "open",
            "tags": [],
            "meta": {}
        });

        let mut result = json!({
            "workspace": workspace.as_str(),
            "type": card_type,
            "supported_types": supported,
            "template": template,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let used = attach_budget(&mut result, limit, false);
            if used > limit {
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("template".to_string(), Value::Null);
                    obj.insert("truncated".to_string(), Value::Bool(true));
                }
                let _ = attach_budget(&mut result, limit, true);
            }
        }

        ai_ok("branchmind_think_template", result)
    }

    fn tool_branchmind_think_card(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch_override = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let trace_doc = match optional_string(args_obj, "trace_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let graph_doc = match optional_string(args_obj, "graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && (trace_doc.is_some() || graph_doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, trace_doc, graph_doc), not both",
            );
        }

        let (branch, trace_doc, graph_doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let branch = branch_override.unwrap_or(reasoning.branch);
                (branch, reasoning.trace_doc, reasoning.graph_doc)
            }
            None => {
                let branch = match branch_override {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let trace_doc = trace_doc.unwrap_or_else(|| DEFAULT_TRACE_DOC.to_string());
                let graph_doc = graph_doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                (branch, trace_doc, graph_doc)
            }
        };

        let supports = match optional_string_array(args_obj, "supports") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let blocks = match optional_string_array(args_obj, "blocks") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };

        let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
        let parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let card_id = match parsed.card_id.clone() {
            Some(id) => id,
            None => match self.store.next_card_id(&workspace) {
                Ok(id) => id,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        };
        let (payload_json, meta_json, content) = build_think_card_payload(
            &card_id,
            &parsed.card_type,
            parsed.title.as_deref(),
            parsed.text.as_deref(),
            &parsed.status,
            &parsed.tags,
            &parsed.meta_value,
        );

        let result = match self.store.think_card_commit(
            &workspace,
            &branch,
            &trace_doc,
            &graph_doc,
            bm_storage::ThinkCardInput {
                card_id: card_id.clone(),
                card_type: parsed.card_type.clone(),
                title: parsed.title.clone(),
                text: parsed.text.clone(),
                status: Some(parsed.status.clone()),
                tags: parsed.tags.clone(),
                meta_json: Some(meta_json),
                content,
                payload_json,
            },
            &supports,
            &blocks,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list or branchmind_branch_create, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) if msg == "unsupported card.type" => {
                let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
                return ai_error_with(
                    "INVALID_INPUT",
                    "Unsupported card.type",
                    Some(&format!(
                        "Supported: {}",
                        supported.iter().copied().collect::<Vec<_>>().join(", ")
                    )),
                    vec![suggest_call(
                        "branchmind_think_template",
                        "Get a valid card skeleton.",
                        "high",
                        json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branchmind_think_card",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "trace_doc": trace_doc,
                "graph_doc": graph_doc,
                "card_id": card_id,
                "inserted": result.inserted,
                "graph_applied": {
                    "nodes_upserted": result.nodes_upserted,
                    "edges_upserted": result.edges_upserted
                },
                "last_seq": result.last_seq
            }),
        )
    }

    fn tool_branchmind_think_context(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch_override = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let graph_doc = match optional_string(args_obj, "graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_some() && graph_doc.is_some() {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, graph_doc), not both",
            );
        }

        let (branch, graph_doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let branch = branch_override.unwrap_or(reasoning.branch);
                (branch, reasoning.graph_doc)
            }
            None => {
                let branch = match branch_override {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let graph_doc = graph_doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                (branch, graph_doc)
            }
        };

        let limit_cards = match optional_usize(args_obj, "limit_cards") {
            Ok(v) => v.unwrap_or(30),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        let slice = match self.store.graph_query(
            &workspace,
            &branch,
            &graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(types),
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: limit_cards,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branchmind_branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branchmind_branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut by_type = std::collections::BTreeMap::<String, u64>::new();
        for n in &slice.nodes {
            *by_type.entry(n.node_type.clone()).or_insert(0) += 1;
        }

        let cards = slice
            .nodes
            .into_iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "type": n.node_type,
                    "title": n.title,
                    "text": n.text,
                    "status": n.status,
                    "tags": n.tags,
                    "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "deleted": n.deleted,
                    "last_seq": n.last_seq,
                    "last_ts_ms": n.last_ts_ms
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "stats": {
                "cards": cards.len(),
                "by_type": by_type
            },
            "cards": cards,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let before = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "cards", limit);
            let after = result
                .get("cards")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("truncated".to_string(), Value::Bool(truncated));
            }
            if after < before {
                let mut by_type = std::collections::BTreeMap::<String, u64>::new();
                if let Some(arr) = result.get("cards").and_then(|v| v.as_array()) {
                    for card in arr {
                        if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
                            *by_type.entry(ty.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                if let Some(stats) = result.get_mut("stats").and_then(|v| v.as_object_mut()) {
                    stats.insert(
                        "cards".to_string(),
                        Value::Number(serde_json::Number::from(after as u64)),
                    );
                    stats.insert("by_type".to_string(), json!(by_type));
                }
            }
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "cards", limit);
                let truncated_final = truncated || truncated2;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_final));
                }
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branchmind_think_context", result)
    }

    fn tool_branchmind_export(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let target_id = match require_string(args_obj, "target") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let notes_limit = match optional_usize(args_obj, "notes_limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let trace_limit = match optional_usize(args_obj, "trace_limit") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let kind = match parse_plan_or_task_kind(&target_id) {
            Some(v) => v,
            None => return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-"),
        };

        let target = match kind {
            TaskKind::Plan => match self.store.get_plan(&workspace, &target_id) {
                Ok(Some(p)) => json!({
                    "id": p.id,
                    "kind": "plan",
                    "revision": p.revision,
                    "title": p.title,
                    "created_at_ms": p.created_at_ms,
                    "updated_at_ms": p.updated_at_ms
                }),
                Ok(None) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown target id",
                        Some("Call tasks_context to discover ids in this workspace, then retry."),
                        vec![suggest_call(
                            "tasks_context",
                            "List plans and tasks for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
            TaskKind::Task => match self.store.get_task(&workspace, &target_id) {
                Ok(Some(t)) => json!({
                    "id": t.id,
                    "kind": "task",
                    "revision": t.revision,
                    "parent": t.parent_plan_id,
                    "title": t.title,
                    "created_at_ms": t.created_at_ms,
                    "updated_at_ms": t.updated_at_ms
                }),
                Ok(None) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown target id",
                        Some("Call tasks_context to discover ids in this workspace, then retry."),
                        vec![suggest_call(
                            "tasks_context",
                            "List plans and tasks for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        };

        let reasoning = match self
            .store
            .ensure_reasoning_ref(&workspace, &target_id, kind)
        {
            Ok(r) => r,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown target id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let branch = reasoning.branch.clone();
        let notes_doc = reasoning.notes_doc.clone();
        let trace_doc = reasoning.trace_doc.clone();

        let notes_slice =
            match self
                .store
                .doc_show_tail(&workspace, &branch, &notes_doc, None, notes_limit)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
        let trace_slice =
            match self
                .store
                .doc_show_tail(&workspace, &branch, &trace_doc, None, trace_limit)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        let notes_entries = notes_slice
            .entries
            .into_iter()
            .map(|e| match e.kind {
                bm_storage::DocEntryKind::Note => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "title": e.title,
                    "format": e.format,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "content": e.content
                }),
                bm_storage::DocEntryKind::Event => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "event_id": e.source_event_id,
                    "event_type": e.event_type,
                    "task_id": e.task_id,
                    "path": e.path
                }),
            })
            .collect::<Vec<_>>();

        let trace_entries = trace_slice
            .entries
            .into_iter()
            .map(|e| match e.kind {
                bm_storage::DocEntryKind::Note => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "title": e.title,
                    "format": e.format,
                    "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "content": e.content
                }),
                bm_storage::DocEntryKind::Event => json!({
                    "seq": e.seq,
                    "ts": ts_ms_to_rfc3339(e.ts_ms),
                    "ts_ms": e.ts_ms,
                    "kind": e.kind.as_str(),
                    "event_id": e.source_event_id,
                    "event_type": e.event_type,
                    "task_id": e.task_id,
                    "path": e.path
                }),
            })
            .collect::<Vec<_>>();

        let notes_count = notes_entries.len();
        let trace_count = trace_entries.len();
        let notes_branch = branch.clone();
        let trace_branch = branch.clone();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "target": target,
            "reasoning_ref": {
                "branch": reasoning.branch,
                "notes_doc": reasoning.notes_doc,
                "graph_doc": reasoning.graph_doc,
                "trace_doc": reasoning.trace_doc
            },
            "notes": {
                "branch": notes_branch,
                "doc": notes_doc,
                "entries": notes_entries,
                "pagination": {
                    "cursor": Value::Null,
                    "next_cursor": notes_slice.next_cursor,
                    "has_more": notes_slice.has_more,
                    "limit": notes_limit,
                    "count": notes_count
                }
            },
            "trace": {
                "branch": trace_branch,
                "doc": trace_doc,
                "entries": trace_entries,
                "pagination": {
                    "cursor": Value::Null,
                    "next_cursor": trace_slice.next_cursor,
                    "has_more": trace_slice.has_more,
                    "limit": trace_limit,
                    "count": trace_count
                }
            },
            "truncated": false
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let mut truncated_any = false;
            for _ in 0..4 {
                let (_used, truncated) = enforce_branchmind_export_budget(&mut result, limit);
                truncated_any = truncated_any || truncated;
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("truncated".to_string(), Value::Bool(truncated_any));
                }
                let used = attach_budget(&mut result, limit, truncated_any);
                if used <= limit {
                    break;
                }
            }
        }

        ai_ok("branchmind_export", result)
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
            "name": "branchmind_init",
            "description": "Initialize workspace storage and bootstrap the default branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_status",
            "description": "Get reasoning store status for a workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_branch_create",
            "description": "Create a new branch ref from an existing branch snapshot (no copy). Defaults to checkout when from is omitted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "name": { "type": "string" },
                    "from": { "type": "string" }
                },
                "required": ["workspace", "name"]
            }
        }),
        json!({
            "name": "branchmind_branch_list",
            "description": "List known branch refs for a workspace (including canonical task/plan branches).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_checkout",
            "description": "Set the current workspace branch ref (does not affect tasks).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "ref": { "type": "string" }
                },
                "required": ["workspace", "ref"]
            }
        }),
        json!({
            "name": "branchmind_notes_commit",
            "description": "Append a note entry to the notes document of a target or an explicit (branch, doc). Defaults to checkout+notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "content": { "type": "string" },
                    "title": { "type": "string" },
                    "format": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "content"]
            }
        }),
        json!({
            "name": "branchmind_show",
            "description": "Read a bounded slice (tail/pagination) of a reasoning document. Defaults to checkout+doc_kind.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "doc_kind": { "type": "string", "enum": ["notes", "trace"] },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_diff",
            "description": "Directional diff between two branches for a single document.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "from", "to"]
            }
        }),
        json!({
            "name": "branchmind_merge",
            "description": "Idempotent merge of note entries from one branch into another (notes VCS).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "from", "into"]
            }
        }),
        json!({
            "name": "branchmind_export",
            "description": "Build a bounded snapshot for fast IDE/agent resumption (target + refs + tail notes/trace).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "target"]
            }
        }),
        json!({
            "name": "branchmind_graph_apply",
            "description": "Apply a batch of typed graph ops to a target graph or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "ops": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": { "type": "string", "enum": ["node_upsert", "node_delete", "edge_upsert", "edge_delete"] },
                                "id": { "type": "string" },
                                "type": { "type": "string" },
                                "title": { "type": "string" },
                                "text": { "type": "string" },
                                "status": { "type": "string" },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "meta": { "type": "object" },
                                "from": { "type": "string" },
                                "rel": { "type": "string" },
                                "to": { "type": "string" }
                            },
                            "required": ["op"]
                        }
                    }
                },
                "required": ["workspace", "ops"]
            }
        }),
        json!({
            "name": "branchmind_graph_query",
            "description": "Query a bounded slice of the effective graph view for a target or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "ids": { "type": "array", "items": { "type": "string" } },
                    "types": { "type": "array", "items": { "type": "string" } },
                    "status": { "type": "string" },
                    "tags_any": { "type": "array", "items": { "type": "string" } },
                    "tags_all": { "type": "array", "items": { "type": "string" } },
                    "text": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "include_edges": { "type": "boolean" },
                    "edges_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_graph_validate",
            "description": "Validate invariants of the effective graph view for a target or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "max_errors": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "branchmind_graph_diff",
            "description": "Directional diff between two branches for a single graph document (patch-style).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "from", "to"]
            }
        }),
        json!({
            "name": "branchmind_graph_merge",
            "description": "Merge graph changes from a derived branch back into its base branch (3-way, conflict-producing).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "from", "into"]
            }
        }),
        json!({
            "name": "branchmind_graph_conflicts",
            "description": "List graph merge conflicts for a destination branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "status": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "into"]
            }
        }),
        json!({
            "name": "branchmind_graph_conflict_show",
            "description": "Show a single conflict with base/theirs/ours snapshots.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "conflict_id": { "type": "string" }
                },
                "required": ["workspace", "conflict_id"]
            }
        }),
        json!({
            "name": "branchmind_graph_conflict_resolve",
            "description": "Resolve a conflict and optionally apply the chosen snapshot into the destination branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "conflict_id": { "type": "string" },
                    "resolution": { "type": "string", "enum": ["use_from", "use_into"] }
                },
                "required": ["workspace", "conflict_id", "resolution"]
            }
        }),
        json!({
            "name": "branchmind_think_template",
            "description": "Return a deterministic thinking card skeleton for a supported type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "type": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "type"]
            }
        }),
        json!({
            "name": "branchmind_think_card",
            "description": "Atomically commit a thinking card into trace_doc and upsert node/edges into graph_doc. Defaults to checkout+docs and auto-id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "card"]
            }
        }),
        json!({
            "name": "branchmind_think_context",
            "description": "Return a bounded low-noise thinking context slice (cards from the graph).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "limit_cards": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
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
            "name": "tasks_close_step",
            "description": "Atomically confirm checkpoints and close a step.",
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
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
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
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
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

fn parse_plan_or_task_kind(id: &str) -> Option<TaskKind> {
    if id.starts_with("PLAN-") {
        Some(TaskKind::Plan)
    } else if id.starts_with("TASK-") {
        Some(TaskKind::Task)
    } else {
        None
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

fn require_checkout_branch(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
) -> Result<String, Value> {
    match store.branch_checkout_get(workspace) {
        Ok(Some(branch)) => Ok(branch),
        Ok(None) => Err(ai_error_with(
            "INVALID_INPUT",
            "Checkout branch is not set",
            Some("Call branchmind_init or branchmind_branch_list to choose a branch."),
            vec![
                suggest_call(
                    "branchmind_init",
                    "Initialize the workspace and bootstrap a default branch.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                ),
                suggest_call(
                    "branchmind_branch_list",
                    "List known branches for this workspace.",
                    "medium",
                    json!({ "workspace": workspace.as_str() }),
                ),
            ],
        )),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
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
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an integer"),
        )),
    }
}

fn optional_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(v) => Ok(Some(v.to_string())),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        )),
    }
}

fn optional_usize(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<usize>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => n.as_u64().map(|v| v as usize).map(Some).ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{key} must be a positive integer"),
            )
        }),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a positive integer"),
        )),
    }
}

fn optional_bool(args: &serde_json::Map<String, Value>, key: &str) -> Result<Option<bool>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Bool(v) => Ok(Some(*v)),
        _ => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a boolean"),
        )),
    }
}

fn optional_step_path(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<StepPath>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(raw) = value.as_str() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        ));
    };
    StepPath::parse(raw)
        .map(Some)
        .map_err(|_| ai_error("INVALID_INPUT", &format!("{key} is invalid")))
}

fn optional_string_array(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(arr) = value.as_array() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an array of strings"),
        ));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{key} must be an array of strings"),
            ));
        };
        out.push(s.to_string());
    }
    Ok(Some(out))
}

fn optional_non_null_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::String(v)) => Ok(Some(v.to_string())),
        Some(Value::Null) => Err(ai_error("INVALID_INPUT", &format!("{key} cannot be null"))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a string"),
        )),
        None => Ok(None),
    }
}

fn optional_nullable_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, Value> {
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
        Some(Value::Object(_)) => Ok(Some(Some(args.get(key).expect("key exists").to_string()))),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an object or null"),
        )),
        None => Ok(None),
    }
}

fn optional_object_as_json_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    if !args.contains_key(key) {
        return Ok(None);
    }
    match args.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::Object(_)) => Ok(Some(args.get(key).expect("key exists").to_string())),
        Some(_) => Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be an object or null"),
        )),
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

#[derive(Clone, Debug)]
struct ParsedThinkCard {
    card_id: Option<String>,
    card_type: String,
    title: Option<String>,
    text: Option<String>,
    status: String,
    tags: Vec<String>,
    meta_value: Value,
}

fn parse_think_card(workspace: &WorkspaceId, value: Value) -> Result<ParsedThinkCard, Value> {
    let raw_obj = match value {
        Value::Object(obj) => obj,
        Value::String(raw) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(ai_error("INVALID_INPUT", "card must not be empty"));
            }
            if raw.starts_with('{') {
                if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) {
                    obj
                } else if looks_like_card_dsl(raw) {
                    parse_think_card_dsl(raw)?
                } else {
                    let mut obj = serde_json::Map::new();
                    obj.insert("text".to_string(), Value::String(raw.to_string()));
                    obj
                }
            } else if looks_like_card_dsl(raw) {
                parse_think_card_dsl(raw)?
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert("text".to_string(), Value::String(raw.to_string()));
                obj
            }
        }
        Value::Null => return Err(ai_error("INVALID_INPUT", "card is required")),
        _ => {
            return Err(ai_error(
                "INVALID_INPUT",
                "card must be an object or string",
            ));
        }
    };

    normalize_think_card(workspace, raw_obj)
}

fn looks_like_card_dsl(raw: &str) -> bool {
    let mut seen = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        seen = true;
        if !line.contains(':') {
            return false;
        }
    }
    seen
}

fn parse_think_card_dsl(raw: &str) -> Result<serde_json::Map<String, Value>, Value> {
    let mut out = serde_json::Map::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(ai_error(
                "INVALID_INPUT",
                "card DSL must be 'key: value' lines",
            ));
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            return Err(ai_error("INVALID_INPUT", "card DSL key must not be empty"));
        }
        out.insert(key.to_string(), Value::String(value.to_string()));
    }
    Ok(out)
}

fn normalize_think_card(
    workspace: &WorkspaceId,
    raw: serde_json::Map<String, Value>,
) -> Result<ParsedThinkCard, Value> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut id: Option<String> = None;
    let mut card_type: Option<String> = None;
    let mut title: Option<String> = None;
    let mut text: Option<String> = None;
    let mut status: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut meta: BTreeMap<String, Value> = BTreeMap::new();

    for (key, value) in raw {
        let key = key.trim().to_ascii_lowercase();
        match key.as_str() {
            "id" | "card_id" => {
                let Some(v) = value.as_str() else {
                    return Err(ai_error("INVALID_INPUT", "card.id must be a string"));
                };
                let v = v.trim();
                if !v.is_empty() {
                    id = Some(v.to_string());
                }
            }
            "type" | "card_type" => {
                let Some(v) = value.as_str() else {
                    return Err(ai_error("INVALID_INPUT", "card.type must be a string"));
                };
                let v = v.trim();
                if !v.is_empty() {
                    card_type = Some(v.to_string());
                }
            }
            "title" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        title = Some(v.to_string());
                    }
                }
            }
            "text" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        text = Some(v.to_string());
                    }
                }
            }
            "status" => {
                if let Some(v) = value.as_str() {
                    let v = v.trim();
                    if !v.is_empty() {
                        status = Some(v.to_string());
                    }
                }
            }
            "tags" => {
                let mut set = BTreeSet::new();
                match value {
                    Value::Array(arr) => {
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err(ai_error(
                                    "INVALID_INPUT",
                                    "card.tags must be an array of strings",
                                ));
                            };
                            let s = s.trim();
                            if !s.is_empty() {
                                set.insert(s.to_lowercase());
                            }
                        }
                    }
                    Value::String(s) => {
                        for part in s.split(|c| c == ';' || c == ',') {
                            let part = part.trim();
                            if !part.is_empty() {
                                set.insert(part.to_lowercase());
                            }
                        }
                    }
                    Value::Null => {}
                    _ => {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            "card.tags must be a string or an array of strings",
                        ));
                    }
                }
                tags = set.into_iter().collect();
            }
            "meta" => match value {
                Value::Object(obj) => {
                    for (k, v) in obj {
                        meta.insert(k, v);
                    }
                }
                Value::String(raw) => {
                    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&raw) {
                        for (k, v) in obj {
                            meta.insert(k, v);
                        }
                    } else {
                        return Err(ai_error("INVALID_INPUT", "card.meta must be an object"));
                    }
                }
                Value::Null => {}
                _ => return Err(ai_error("INVALID_INPUT", "card.meta must be an object")),
            },
            _ => {
                meta.insert(key, value);
            }
        }
    }

    let card_id = id;
    let card_type = card_type.unwrap_or_else(|| "note".to_string());
    if !bm_core::think::is_supported_think_card_type(&card_type) {
        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        return Err(ai_error_with(
            "INVALID_INPUT",
            "Unsupported card.type",
            Some(&format!(
                "Supported: {}",
                supported.iter().copied().collect::<Vec<_>>().join(", ")
            )),
            vec![suggest_call(
                "branchmind_think_template",
                "Get a valid card skeleton.",
                "high",
                json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
            )],
        ));
    }

    if title.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
        && text.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
    {
        return Err(ai_error(
            "INVALID_INPUT",
            "card must have at least one of title or text",
        ));
    }

    let status = status.unwrap_or_else(|| "open".to_string());
    let meta_value = Value::Object(meta.into_iter().collect());

    Ok(ParsedThinkCard {
        card_id,
        card_type,
        title,
        text,
        status,
        tags,
        meta_value,
    })
}

fn build_think_card_payload(
    card_id: &str,
    card_type: &str,
    title: Option<&str>,
    text: Option<&str>,
    status: &str,
    tags: &[String],
    meta_value: &Value,
) -> (String, String, String) {
    let normalized = json!({
        "id": card_id,
        "type": card_type,
        "title": title,
        "text": text,
        "status": status,
        "tags": tags,
        "meta": meta_value.clone()
    });
    let payload_json = normalized.to_string();
    let meta_json = json!({
        "source": "think_card",
        "card_id": card_id,
        "type": card_type,
        "status": status,
        "tags": tags,
        "meta": meta_value.clone()
    })
    .to_string();
    let content = text
        .map(|s| s.to_string())
        .or_else(|| title.map(|s| s.to_string()))
        .unwrap_or_default();
    (payload_json, meta_json, content)
}

fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::RevisionMismatch { expected, actual } => {
            format!("Revision mismatch: expected={expected} actual={actual}")
        }
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::UnknownBranch => "Unknown branch".to_string(),
        StoreError::UnknownConflict => "Unknown conflict".to_string(),
        StoreError::ConflictAlreadyResolved => "Conflict already resolved".to_string(),
        StoreError::MergeNotSupported => "Merge not supported".to_string(),
        StoreError::BranchAlreadyExists => "Branch already exists".to_string(),
        StoreError::BranchCycle => "Branch base cycle".to_string(),
        StoreError::BranchDepthExceeded => "Branch base depth exceeded".to_string(),
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

fn ai_error_with(
    code: &str,
    message: &str,
    recovery: Option<&str>,
    suggestions: Vec<Value>,
) -> Value {
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

fn enforce_branchmind_show_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if value.get("entries").is_some() {
        if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries.iter_mut() {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    continue;
                }
                let Some(content) = entry
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                else {
                    continue;
                };
                let shorter = truncate_string(&content, 256);
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("content".to_string(), Value::String(shorter));
                }
            }
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }

        if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries.iter_mut() {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    continue;
                }
                if let Some(obj) = entry.as_object_mut() {
                    if obj.contains_key("meta") {
                        obj.insert("meta".to_string(), Value::Null);
                    }
                }
            }
        }
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }

        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                return (used, truncated);
            }
            let removed =
                if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
                    if entries.is_empty() {
                        false
                    } else {
                        entries.remove(0);
                        true
                    }
                } else {
                    false
                };
            if !removed {
                break;
            }
            truncated = true;
        }
    }

    if value.get("last_doc_entry").is_some() {
        if let Some(obj) = value.as_object_mut() {
            obj.remove("last_doc_entry");
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    if value.get("last_event").is_some() {
        if let Some(obj) = value.as_object_mut() {
            obj.remove("last_event");
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    (used, truncated)
}

fn enforce_branchmind_branch_list_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if value.get("branches").is_some() {
        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                break;
            }
            let removed =
                if let Some(branches) = value.get_mut("branches").and_then(|v| v.as_array_mut()) {
                    if branches.is_empty() {
                        false
                    } else {
                        branches.remove(0);
                        true
                    }
                } else {
                    false
                };
            if !removed {
                break;
            }
            truncated = true;
        }
    }

    (used, truncated)
}

fn enforce_graph_list_budget(value: &mut Value, list_key: &str, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;
    if value.get(list_key).is_some() {
        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                break;
            }
            let removed = if let Some(arr) = value.get_mut(list_key).and_then(|v| v.as_array_mut())
            {
                arr.pop().is_some()
            } else {
                false
            };
            if !removed {
                break;
            }
            truncated = true;
        }
    }

    (used, truncated)
}

fn enforce_graph_query_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    use std::collections::HashSet;

    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    loop {
        used = json_len_chars(value);
        if used <= max_chars {
            break;
        }

        let removed_edge =
            if let Some(edges) = value.get_mut("edges").and_then(|v| v.as_array_mut()) {
                edges.pop().is_some()
            } else {
                false
            };
        if removed_edge {
            truncated = true;
            continue;
        }

        let removed_node =
            if let Some(nodes) = value.get_mut("nodes").and_then(|v| v.as_array_mut()) {
                nodes.pop().is_some()
            } else {
                false
            };
        if removed_node {
            truncated = true;

            let mut node_ids = HashSet::new();
            if let Some(nodes) = value.get("nodes").and_then(|v| v.as_array()) {
                for node in nodes {
                    if let Some(id) = node.get("id").and_then(|v| v.as_str()) {
                        node_ids.insert(id.to_string());
                    }
                }
            }

            if let Some(edges) = value.get_mut("edges").and_then(|v| v.as_array_mut()) {
                edges.retain(|edge| {
                    let from = edge.get("from").and_then(|v| v.as_str()).unwrap_or("");
                    let to = edge.get("to").and_then(|v| v.as_str()).unwrap_or("");
                    node_ids.contains(from) && node_ids.contains(to)
                });
            }

            continue;
        }

        break;
    }

    (used, truncated)
}

fn export_pop_first_entry(value: &mut Value, section_key: &str) -> bool {
    let Some(section) = value.get_mut(section_key).and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let Some(entries) = section.get_mut("entries").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    if entries.is_empty() {
        return false;
    }

    entries.remove(0);
    let count = entries.len();
    if let Some(pagination) = section
        .get_mut("pagination")
        .and_then(|v| v.as_object_mut())
    {
        pagination.insert(
            "count".to_string(),
            Value::Number(serde_json::Number::from(count as u64)),
        );
    }
    true
}

fn enforce_branchmind_export_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    // First pass: shrink note content/meta (high-yield) in both sections.
    for section_key in ["notes", "trace"] {
        if let Some(section) = value.get_mut(section_key).and_then(|v| v.as_object_mut()) {
            if let Some(entries) = section.get_mut("entries").and_then(|v| v.as_array_mut()) {
                for entry in entries.iter_mut() {
                    if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                        continue;
                    }
                    let Some(content) = entry
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                    else {
                        continue;
                    };
                    let shorter = truncate_string(&content, 256);
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("content".to_string(), Value::String(shorter));
                        if obj.contains_key("meta") {
                            obj.insert("meta".to_string(), Value::Null);
                        }
                    }
                }
            }
        }
    }

    let truncated = true;
    used = json_len_chars(value);
    if used <= max_chars {
        return (used, truncated);
    }

    // Second pass: drop oldest entries (prefer notes, then trace) until within budget.
    loop {
        used = json_len_chars(value);
        if used <= max_chars {
            break;
        }
        if export_pop_first_entry(value, "notes") {
            continue;
        }
        if export_pop_first_entry(value, "trace") {
            continue;
        }
        break;
    }

    used = json_len_chars(value);
    if used <= max_chars {
        return (used, truncated);
    }

    // Third pass: remove non-essential fields in the nested payloads.
    for section_key in ["notes", "trace"] {
        if let Some(section) = value.get_mut(section_key).and_then(|v| v.as_object_mut()) {
            section.remove("pagination");
            section.remove("branch");
            section.remove("doc");
        }
    }

    used = json_len_chars(value);
    if used <= max_chars {
        return (used, truncated);
    }

    if let Some(target) = value.get_mut("target").and_then(|v| v.as_object_mut()) {
        target.remove("created_at_ms");
        target.remove("updated_at_ms");
        target.remove("parent");
    }
    if let Some(refs) = value
        .get_mut("reasoning_ref")
        .and_then(|v| v.as_object_mut())
    {
        refs.remove("graph_doc");
    }

    used = json_len_chars(value);
    if used <= max_chars {
        return (used, truncated);
    }

    if let Some(obj) = value.as_object_mut() {
        obj.remove("trace");
    }
    used = json_len_chars(value);
    if used <= max_chars {
        return (used, truncated);
    }

    if let Some(obj) = value.as_object_mut() {
        obj.remove("notes");
    }
    used = json_len_chars(value);
    (used, truncated)
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

fn attach_budget(value: &mut Value, max_chars: usize, truncated: bool) -> usize {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "budget".to_string(),
            json!({
                "max_chars": max_chars,
                "used_chars": 0,
                "truncated": truncated
            }),
        );
    }

    let mut used = json_len_chars(value);
    for _ in 0..4 {
        if let Some(obj) = value.as_object_mut() {
            if let Some(budget) = obj.get_mut("budget").and_then(|v| v.as_object_mut()) {
                budget.insert(
                    "used_chars".to_string(),
                    Value::Number(serde_json::Number::from(used as u64)),
                );
                budget.insert("truncated".to_string(), Value::Bool(truncated));
            }
        }
        let next = json_len_chars(value);
        if next == used {
            break;
        }
        used = next;
    }

    used
}

const SENSITIVE_KEYWORDS: [&str; 8] = [
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "authorization",
    "bearer",
];

fn redact_value(value: &mut Value, depth: usize) {
    if depth == 0 {
        return;
    }
    match value {
        Value::String(s) => {
            let redacted = redact_text(s);
            if &redacted != s {
                *s = redacted;
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item, depth - 1);
            }
        }
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if is_sensitive_key(&key) {
                    map.insert(key, Value::String("<redacted>".to_string()));
                } else if let Some(value) = map.get_mut(&key) {
                    redact_value(value, depth - 1);
                }
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYWORDS
        .iter()
        .any(|token| lower.contains(token))
}

fn redact_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut out = text.to_string();
    out = redact_token_prefix(&out, "ghp_", 20);
    out = redact_token_prefix(&out, "github_pat_", 20);
    out = redact_token_prefix(&out, "sk-", 20);
    for key in ["token", "apikey", "api_key", "secret", "password"] {
        out = redact_query_param(&out, key);
    }
    out = redact_bearer_token(&out);
    out = redact_private_key_block(&out);
    out
}

fn redact_token_prefix(input: &str, prefix: &str, min_tail: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    while i < bytes.len() {
        if bytes[i..].starts_with(prefix_bytes) {
            let start = i;
            let mut j = i + prefix_bytes.len();
            while j < bytes.len() && is_token_char(bytes[j]) {
                j += 1;
            }
            if j - start >= prefix_bytes.len() + min_tail {
                out.push_str("<redacted>");
            } else {
                out.push_str(&input[start..j]);
            }
            i = j;
        } else {
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn redact_query_param(input: &str, key: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let pattern = format!("{key}=");
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(pos) = lower[i..].find(&pattern) {
        let start = i + pos;
        let value_start = start + pattern.len();
        out.push_str(&input[i..value_start]);
        let mut j = value_start;
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_whitespace() || b == b'&' || b == b';' {
                break;
            }
            j += 1;
        }
        out.push_str("<redacted>");
        i = j;
    }
    out.push_str(&input[i..]);
    out
}

fn redact_bearer_token(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let needle = "bearer ";
    while let Some(pos) = lower[i..].find(needle) {
        let start = i + pos;
        let token_start = start + needle.len();
        out.push_str(&input[i..token_start]);
        let mut j = token_start;
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_whitespace() {
                break;
            }
            j += 1;
        }
        out.push_str("<redacted>");
        i = j;
    }
    out.push_str(&input[i..]);
    out
}

fn redact_private_key_block(input: &str) -> String {
    if !input.contains("PRIVATE KEY") {
        return input.to_string();
    }
    let begin = "-----BEGIN ";
    let end = "-----END ";
    let Some(start) = input.find(begin) else {
        return "<redacted>".to_string();
    };
    let Some(end_pos) = input[start..].find(end) else {
        return "<redacted>".to_string();
    };
    let end_abs = start + end_pos;
    let end_line = input[end_abs..]
        .find("-----")
        .map(|p| end_abs + p + 5)
        .unwrap_or(input.len());
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..start]);
    out.push_str("<redacted>");
    out.push_str(&input[end_line..]);
    out
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
