#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{SqliteStore, StoreError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

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
            "tasks_edit" => self.tool_tasks_edit(args),
            "tasks_context" => self.tool_tasks_context(args),
            "tasks_delta" => self.tool_tasks_delta(args),
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
                ai_error("REVISION_MISMATCH", &format!("expected={expected} actual={actual}"))
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
                    "ts_ms": e.ts_ms,
                    "task": e.task_id,
                    "path": e.path,
                    "type": e.event_type,
                    "payload": parse_json_or_string(&e.payload_json),
                })).collect::<Vec<_>>()
            }),
        )
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
    }
}

fn ai_ok(intent: &str, result: Value) -> Value {
    json!({
        "success": true,
        "intent": intent,
        "result": result,
        "warnings": [],
        "suggestions": [],
        "context": {},
        "error": null,
        "timestamp": now_rfc3339_fallback_ms(),
    })
}

fn ai_error(code: &str, message: &str) -> Value {
    json!({
        "success": false,
        "intent": "error",
        "result": {},
        "warnings": [],
        "suggestions": [],
        "context": {},
        "error": { "code": code, "message": message },
        "timestamp": now_rfc3339_fallback_ms(),
    })
}

fn now_rfc3339_fallback_ms() -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // v0 skeleton: use a numeric timestamp to avoid pulling time formatting deps early.
    Value::Number(serde_json::Number::from(now.as_millis() as u64))
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
