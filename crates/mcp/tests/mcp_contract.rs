#![forbid(unsafe_code)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    storage_dir: PathBuf,
}

impl Server {
    fn start(test_name: &str) -> Self {
        let storage_dir = temp_dir(test_name);
        let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--storage-dir")
            .arg(&storage_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn bm_mcp");

        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));

        Self {
            child,
            stdin,
            stdout,
            storage_dir,
        }
    }

    fn send(&mut self, req: Value) {
        writeln!(self.stdin, "{}", req.to_string()).expect("write request");
        self.stdin.flush().expect("flush request");
    }

    fn recv(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(!line.trim().is_empty(), "empty response line");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn request(&mut self, req: Value) -> Value {
        self.send(req);
        self.recv()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.storage_dir);
    }
}

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_mcp_{test_name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn extract_tool_text(resp: &Value) -> Value {
    let text = resp
        .get("result")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .expect("result.content[0].text");
    serde_json::from_str(text).expect("parse tool text json")
}

fn assert_json_rpc_error(resp: &Value, expected_code: i64) {
    let code = resp
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_i64())
        .expect("error.code");
    assert_eq!(code, expected_code);
}

#[test]
fn mcp_requires_notifications_initialized() {
    let mut server = Server::start("requires_initialized");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert!(init.get("result").is_some(), "initialize must return result");

    let tools_list_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    assert_json_rpc_error(&tools_list_before, -32002);

    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list = server.request(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let mut names = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["storage", "tasks_context", "tasks_create", "tasks_delta", "tasks_edit"]
    );
}

#[test]
fn tasks_create_context_delta_smoke() {
    let mut server = Server::start("tasks_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
    }));
    assert_eq!(
        created_plan
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    assert_eq!(
        created_task_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let edited_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_edit", "arguments": { "workspace": "ws1", "task": plan_id, "expected_revision": 0, "title": "Plan B" } }
    }));
    let edited_text = extract_tool_text(&edited_plan);
    assert_eq!(
        edited_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        edited_text
            .get("result")
            .and_then(|v| v.get("revision"))
            .and_then(|v| v.as_i64()),
        Some(1)
    );

    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws1" } }
    }));
    let ctx_text = extract_tool_text(&context);
    let plans = ctx_text
        .get("result")
        .and_then(|v| v.get("plans"))
        .and_then(|v| v.as_array())
        .expect("plans");
    let tasks = ctx_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
        .expect("tasks");
    assert_eq!(plans.len(), 1);
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        plans[0].get("title").and_then(|v| v.as_str()),
        Some("Plan B")
    );
    assert_eq!(
        plans[0].get("revision").and_then(|v| v.as_i64()),
        Some(1)
    );

    let delta = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1" } }
    }));
    let delta_text = extract_tool_text(&delta);
    let events = delta_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 3);
}

#[test]
fn tasks_edit_revision_mismatch() {
    let mut server = Server::start("tasks_edit_revision_mismatch");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let mismatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_edit", "arguments": { "workspace": "ws1", "task": plan_id, "expected_revision": 999, "title": "Nope" } }
    }));
    assert_eq!(
        mismatch
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let mismatch_text = extract_tool_text(&mismatch);
    assert_eq!(
        mismatch_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("REVISION_MISMATCH")
    );

    let delta = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1" } }
    }));
    let delta_text = extract_tool_text(&delta);
    let events = delta_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 1);
}
