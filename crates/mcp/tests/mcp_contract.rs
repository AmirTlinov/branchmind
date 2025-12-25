#![forbid(unsafe_code)]

use serde_json::{Value, json};
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
    assert!(
        init.get("result").is_some(),
        "initialize must return result"
    );

    let tools_list_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    assert_json_rpc_error(&tools_list_before, -32002);

    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let mut names = tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "branchmind_branch_create",
            "branchmind_branch_list",
            "branchmind_checkout",
            "branchmind_export",
            "branchmind_init",
            "branchmind_notes_commit",
            "branchmind_show",
            "branchmind_status",
            "storage",
            "tasks_context",
            "tasks_create",
            "tasks_decompose",
            "tasks_define",
            "tasks_delta",
            "tasks_done",
            "tasks_edit",
            "tasks_focus_clear",
            "tasks_focus_get",
            "tasks_focus_set",
            "tasks_note",
            "tasks_radar",
            "tasks_verify",
        ]
    );
}

#[test]
fn branchmind_notes_and_trace_ingestion_smoke() {
    let mut server = Server::start("branchmind_memory_smoke");

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

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws1", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    assert_eq!(
        decompose_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let task_id = decompose_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let show_trace = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "trace", "limit": 50 } }
    }));
    let trace_text = extract_tool_text(&show_trace);
    let entries = trace_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("task_created")),
        "trace must contain task_created"
    );
    assert!(
        entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("steps_added")),
        "trace must contain steps_added"
    );

    let long_note = "x".repeat(2048);
    let notes_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } }
    }));
    let notes_commit_text = extract_tool_text(&notes_commit);
    assert_eq!(
        notes_commit_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let show_notes_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "notes", "limit": 50, "max_chars": 400 } }
    }));
    let notes_text = extract_tool_text(&show_notes_budget);
    assert_eq!(
        notes_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let truncated = notes_text
        .get("result")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected truncated=true with small max_chars");
    let used = notes_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .and_then(|v| v.get("used_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(9999);
    assert!(used <= 400, "budget.used_chars must not exceed max_chars");
}

#[test]
fn branchmind_branching_inherits_base_snapshot() {
    let mut server = Server::start("branchmind_branching_inherits_base_snapshot");

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

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let canonical_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let notes_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("notes_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.notes_doc")
        .to_string();

    let checkout = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_checkout", "arguments": { "workspace": "ws1", "ref": canonical_branch.clone() } }
    }));
    let checkout_text = extract_tool_text(&checkout);
    assert_eq!(
        checkout_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        checkout_text
            .get("result")
            .and_then(|v| v.get("current"))
            .and_then(|v| v.as_str()),
        Some(canonical_branch.as_str())
    );

    let base_note_content = "base note";
    let base_note_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": base_note_content } }
    }));
    let base_note_commit_text = extract_tool_text(&base_note_commit);
    assert_eq!(
        base_note_commit_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let derived_branch = format!("{}/alt", canonical_branch);
    let branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_create", "arguments": { "workspace": "ws1", "name": derived_branch.clone() } }
    }));
    let branch_create_text = extract_tool_text(&branch_create);
    assert_eq!(
        branch_create_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        branch_create_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.get("base_branch"))
            .and_then(|v| v.as_str()),
        Some(canonical_branch.as_str())
    );

    let derived_note_content = "derived note";
    let derived_note_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "content": derived_note_content } }
    }));
    let derived_note_commit_text = extract_tool_text(&derived_note_commit);
    assert_eq!(
        derived_note_commit_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let show_derived = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } }
    }));
    let derived_text = extract_tool_text(&show_derived);
    let derived_entries = derived_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        derived_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(base_note_content) }),
        "derived view must include base note"
    );
    assert!(
        derived_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content) }),
        "derived view must include derived note"
    );

    let show_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "branch": canonical_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } }
    }));
    let base_text = extract_tool_text(&show_base);
    let base_entries = base_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        base_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(base_note_content)),
        "base view must include base note"
    );
    assert!(
        !base_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content) }),
        "base view must not include derived note"
    );

    let branch_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_list", "arguments": { "workspace": "ws1", "limit": 200 } }
    }));
    let branch_list_text = extract_tool_text(&branch_list);
    let branches = branch_list_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .expect("branches");
    assert!(
        branches
            .iter()
            .any(|b| b.get("name").and_then(|v| v.as_str()) == Some(canonical_branch.as_str())),
        "branch list must include canonical branch"
    );
    assert!(
        branches
            .iter()
            .any(|b| b.get("name").and_then(|v| v.as_str()) == Some(derived_branch.as_str())),
        "branch list must include derived branch"
    );
}

#[test]
fn branchmind_export_smoke() {
    let mut server = Server::start("branchmind_export_smoke");

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

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let note_content = "export note";
    let notes_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": note_content } }
    }));
    let notes_commit_text = extract_tool_text(&notes_commit);
    assert_eq!(
        notes_commit_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_export", "arguments": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50 } }
    }));
    let export_text = extract_tool_text(&export);
    assert_eq!(
        export_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let notes_entries = export_text
        .get("result")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(
        notes_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "export must include the note in notes.entries"
    );

    let trace_entries = export_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace.entries");
    assert!(
        trace_entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("task_created")),
        "export must include task_created in trace.entries"
    );

    let long_note = "x".repeat(2048);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } }
    }));

    let export_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_export", "arguments": { "workspace": "ws1", "target": task_id, "notes_limit": 50, "trace_limit": 50, "max_chars": 400 } }
    }));
    let export_budget_text = extract_tool_text(&export_budget);
    assert_eq!(
        export_budget_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let truncated = export_budget_text
        .get("result")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected truncated=true with small max_chars");
    let used = export_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .and_then(|v| v.get("used_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(9999);
    assert!(used <= 400, "budget.used_chars must not exceed max_chars");
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
        created_task_text.get("success").and_then(|v| v.as_bool()),
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
    assert_eq!(plans[0].get("revision").and_then(|v| v.as_i64()), Some(1));

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
    let suggestions = mismatch_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .expect("suggestions");
    assert!(
        !suggestions.is_empty(),
        "REVISION_MISMATCH must include suggestions"
    );
    assert_eq!(
        suggestions[0].get("target").and_then(|v| v.as_str()),
        Some("tasks_context")
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

#[test]
fn tasks_focus_and_radar_smoke() {
    let mut server = Server::start("tasks_focus_and_radar_smoke");

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

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let focus_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws1" } }
    }));
    let focus_before_text = extract_tool_text(&focus_before);
    assert_eq!(
        focus_before_text.get("result").and_then(|v| v.get("focus")),
        Some(&Value::Null)
    );

    let radar_without_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1" } }
    }));
    assert_eq!(
        radar_without_focus
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let radar_without_focus_text = extract_tool_text(&radar_without_focus);
    assert_eq!(
        radar_without_focus_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
    let suggestions = radar_without_focus_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .expect("suggestions");
    assert_eq!(
        suggestions[0].get("target").and_then(|v| v.as_str()),
        Some("tasks_context")
    );

    let focus_set = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));
    let focus_set_text = extract_tool_text(&focus_set);
    assert_eq!(
        focus_set_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );

    let radar_from_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1", "max_chars": 10 } }
    }));
    assert_eq!(
        radar_from_focus
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    let radar_from_focus_text = extract_tool_text(&radar_from_focus);
    let expected_branch = format!("task/{task_id}");
    assert_eq!(
        radar_from_focus_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert_eq!(
        radar_from_focus_text
            .get("result")
            .and_then(|v| v.get("reasoning_ref"))
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(expected_branch.as_str())
    );
    assert_eq!(
        radar_from_focus_text
            .get("result")
            .and_then(|v| v.get("budget"))
            .and_then(|v| v.get("max_chars"))
            .and_then(|v| v.as_u64()),
        Some(10)
    );

    let focus_cleared = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_focus_clear", "arguments": { "workspace": "ws1" } }
    }));
    let focus_cleared_text = extract_tool_text(&focus_cleared);
    assert_eq!(
        focus_cleared_text
            .get("result")
            .and_then(|v| v.get("cleared"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn tasks_steps_gated_done_and_radar() {
    let mut server = Server::start("tasks_steps_gated_done_and_radar");

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

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));

    let decomposed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_decompose",
            "arguments": {
                "workspace": "ws1",
                "task": task_id,
                "steps": [
                    { "title": "Step 1", "success_criteria": ["ok"] }
                ]
            }
        }
    }));
    let decomposed_text = extract_tool_text(&decomposed);
    let step_id = decomposed_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step_id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1" } }
    }));
    let radar_text = extract_tool_text(&radar);
    let focused_task_id = radar_text
        .get("result")
        .and_then(|v| v.get("target"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("radar target id")
        .to_string();
    let verify = radar_text
        .get("result")
        .and_then(|v| v.get("radar"))
        .and_then(|v| v.get("verify"))
        .and_then(|v| v.as_array())
        .expect("radar.verify");
    assert!(
        !verify.is_empty(),
        "radar.verify must reflect missing checkpoints"
    );

    let done_without_verify = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws1", "task": focused_task_id.clone(), "step_id": step_id.clone() } }
    }));
    assert_eq!(
        done_without_verify
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let done_text = extract_tool_text(&done_without_verify);
    assert_eq!(
        done_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("CHECKPOINTS_NOT_CONFIRMED")
    );
    let suggestions = done_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .expect("suggestions");
    assert_eq!(
        suggestions[0].get("target").and_then(|v| v.as_str()),
        Some("tasks_verify")
    );

    let verify_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_verify", "arguments": { "workspace": "ws1", "task": focused_task_id.clone(), "step_id": step_id.clone(), "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } } } }
    }));
    let verify_step_text = extract_tool_text(&verify_step);
    assert_eq!(
        verify_step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws1", "task": focused_task_id, "step_id": step_id } }
    }));
    let done_text2 = extract_tool_text(&done);
    assert_eq!(
        done_text2.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}
