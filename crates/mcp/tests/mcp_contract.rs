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
            "branchmind_branch_delete",
            "branchmind_branch_list",
            "branchmind_branch_rename",
            "branchmind_checkout",
            "branchmind_commit",
            "branchmind_context_pack",
            "branchmind_diff",
            "branchmind_docs_list",
            "branchmind_export",
            "branchmind_graph_apply",
            "branchmind_graph_conflict_resolve",
            "branchmind_graph_conflict_show",
            "branchmind_graph_conflicts",
            "branchmind_graph_diff",
            "branchmind_graph_merge",
            "branchmind_graph_query",
            "branchmind_graph_validate",
            "branchmind_init",
            "branchmind_log",
            "branchmind_merge",
            "branchmind_notes_commit",
            "branchmind_reflog",
            "branchmind_reset",
            "branchmind_show",
            "branchmind_status",
            "branchmind_tag_create",
            "branchmind_tag_delete",
            "branchmind_tag_list",
            "branchmind_think_add_decision",
            "branchmind_think_add_evidence",
            "branchmind_think_add_frame",
            "branchmind_think_add_hypothesis",
            "branchmind_think_add_note",
            "branchmind_think_add_question",
            "branchmind_think_add_test",
            "branchmind_think_add_update",
            "branchmind_think_card",
            "branchmind_think_context",
            "branchmind_think_frontier",
            "branchmind_think_link",
            "branchmind_think_lint",
            "branchmind_think_next",
            "branchmind_think_nominal_merge",
            "branchmind_think_pack",
            "branchmind_think_pin",
            "branchmind_think_pins",
            "branchmind_think_pipeline",
            "branchmind_think_playbook",
            "branchmind_think_query",
            "branchmind_think_set_status",
            "branchmind_think_subgoal_close",
            "branchmind_think_subgoal_open",
            "branchmind_think_template",
            "branchmind_think_watch",
            "branchmind_trace_hydrate",
            "branchmind_trace_sequential_step",
            "branchmind_trace_step",
            "branchmind_trace_validate",
            "storage",
            "tasks_batch",
            "tasks_block",
            "tasks_bootstrap",
            "tasks_close_step",
            "tasks_complete",
            "tasks_context",
            "tasks_context_pack",
            "tasks_contract",
            "tasks_create",
            "tasks_decompose",
            "tasks_define",
            "tasks_delete",
            "tasks_delta",
            "tasks_done",
            "tasks_edit",
            "tasks_evidence_capture",
            "tasks_focus_clear",
            "tasks_focus_get",
            "tasks_focus_set",
            "tasks_handoff",
            "tasks_history",
            "tasks_lint",
            "tasks_macro_close_step",
            "tasks_macro_finish",
            "tasks_macro_start",
            "tasks_mirror",
            "tasks_note",
            "tasks_patch",
            "tasks_plan",
            "tasks_progress",
            "tasks_radar",
            "tasks_redo",
            "tasks_resume",
            "tasks_resume_pack",
            "tasks_resume_super",
            "tasks_scaffold",
            "tasks_storage",
            "tasks_task_add",
            "tasks_task_define",
            "tasks_task_delete",
            "tasks_templates_list",
            "tasks_undo",
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

    let secret_note = "Authorization: Bearer sk-THISISSECRET0123456789012345 token=supersecret";
    let long_note = format!("{secret_note} {}", "x".repeat(2048));
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

    let show_notes = server.request(json!({
        "jsonrpc": "2.0",
        "id": 70,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "notes", "limit": 50 } }
    }));
    let show_notes_text = extract_tool_text(&show_notes);
    let note_entries = show_notes_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    let note_content = note_entries
        .iter()
        .find(|e| e.get("kind").and_then(|v| v.as_str()) == Some("note"))
        .and_then(|e| e.get("content"))
        .and_then(|v| v.as_str())
        .expect("note content");
    assert!(!note_content.contains("sk-THISISSECRET"));
    assert!(note_content.contains("<redacted>"));

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
fn tasks_create_with_steps_sets_fields() {
    let mut server = Server::start("tasks_create_steps");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_steps", "kind": "plan", "title": "Plan Steps" } }
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
        "params": { "name": "tasks_create", "arguments": {
            "workspace": "ws_steps",
            "kind": "task",
            "parent": plan_id.clone(),
            "title": "Task Steps",
            "steps": [
                {
                    "title": "Step A",
                    "success_criteria": ["c1"],
                    "tests": ["t1"],
                    "blockers": ["b1"]
                }
            ]
        } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume", "arguments": { "workspace": "ws_steps", "task": task_id.clone() } }
    }));
    let resume_text = extract_tool_text(&resume);
    let steps = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    let step = steps.first().expect("step");
    let tests = step.get("tests").and_then(|v| v.as_array()).expect("tests");
    assert!(
        tests.iter().any(|v| v.as_str() == Some("t1")),
        "tests should include t1"
    );
    let blockers = step
        .get("blockers")
        .and_then(|v| v.as_array())
        .expect("blockers");
    assert!(
        blockers.iter().any(|v| v.as_str() == Some("b1")),
        "blockers should include b1"
    );
}

#[test]
fn tasks_templates_list_smoke() {
    let mut server = Server::start("tasks_templates_list");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_templates_list", "arguments": { "workspace": "ws_templates" } }
    }));
    let list_text = extract_tool_text(&list);
    let templates = list_text
        .get("result")
        .and_then(|v| v.get("templates"))
        .and_then(|v| v.as_array())
        .expect("templates");
    assert!(
        templates
            .iter()
            .any(|t| t.get("id").and_then(|v| v.as_str()) == Some("basic-task")),
        "templates_list should include basic-task"
    );
}

#[test]
fn tasks_scaffold_task_smoke() {
    let mut server = Server::start("tasks_scaffold");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let scaffold = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_scaffold",
            "arguments": {
                "workspace": "ws_scaffold",
                "template": "basic-task",
                "kind": "task",
                "title": "Scaffold Task",
                "plan_title": "Scaffold Plan"
            }
        }
    }));
    let scaffold_text = extract_tool_text(&scaffold);
    assert_eq!(
        scaffold_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let steps = scaffold_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    assert!(!steps.is_empty(), "scaffold should create steps");
}

#[test]
fn branchmind_bootstrap_defaults() {
    let mut server = Server::start("branchmind_bootstrap_defaults");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "branchmind_init", "arguments": { "workspace": "ws_boot" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let branch_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_list", "arguments": { "workspace": "ws_boot", "limit": 50 } }
    }));
    let branch_list_text = extract_tool_text(&branch_list);
    let branches = branch_list_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .expect("branches");
    let has_main = branches
        .iter()
        .any(|b| b.get("name").and_then(|v| v.as_str()) == Some("main"));
    assert!(has_main, "default branch main should exist");

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws_boot", "content": "hello" } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        note_text
            .get("result")
            .and_then(|v| v.get("entry"))
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        note_text
            .get("result")
            .and_then(|v| v.get("entry"))
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some("notes")
    );

    let show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws_boot", "doc_kind": "notes", "limit": 10 } }
    }));
    let show_text = extract_tool_text(&show);
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some("notes")
    );
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
fn branchmind_diff_and_merge_notes_smoke() {
    let mut server = Server::start("branchmind_diff_and_merge_notes_smoke");

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

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_checkout", "arguments": { "workspace": "ws1", "ref": canonical_branch.clone() } }
    }));

    let base_note_content = "base note";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": base_note_content } }
    }));

    let derived_branch = format!("{}/alt2", canonical_branch);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_create", "arguments": { "workspace": "ws1", "name": derived_branch.clone() } }
    }));

    let derived_note_content = "derived note";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "content": derived_note_content } }
    }));

    let diff = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_diff", "arguments": { "workspace": "ws1", "from": canonical_branch.clone(), "to": derived_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } }
    }));
    let diff_text = extract_tool_text(&diff);
    assert_eq!(
        diff_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let diff_entries = diff_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("diff entries");
    assert!(
        diff_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content)),
        "diff(base→derived) must include derived note"
    );
    assert!(
        !diff_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(base_note_content)),
        "diff(base→derived) must not include base note"
    );

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_merge", "arguments": { "workspace": "ws1", "from": derived_branch.clone(), "into": canonical_branch.clone(), "doc": notes_doc.clone() } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let merged_count = merge_text
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(merged_count, 1, "first merge must merge exactly one note");

    let merge2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "branchmind_merge", "arguments": { "workspace": "ws1", "from": derived_branch.clone(), "into": canonical_branch.clone(), "doc": notes_doc.clone() } }
    }));
    let merge2_text = extract_tool_text(&merge2);
    assert_eq!(
        merge2_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let merged_count2 = merge2_text
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_u64())
        .unwrap_or(999);
    assert_eq!(
        merged_count2, 0,
        "second merge must be idempotent (merged=0)"
    );

    let show_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "branch": canonical_branch, "doc": notes_doc, "limit": 50 } }
    }));
    let show_base_text = extract_tool_text(&show_base);
    let base_entries = show_base_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        base_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content)),
        "base view must include merged derived note after merge"
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

    let context_card_id = "CARD-CONTEXT-PACK-1";
    let context_card_text = "context pack smoke";
    let think_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_think_card", "arguments": { "workspace": "ws1", "target": task_id.clone(), "card": { "id": context_card_id, "type": "note", "title": "Context pack", "text": context_card_text } } }
    }));
    let think_card_text = extract_tool_text(&think_card);
    assert_eq!(
        think_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let context_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_context_pack", "arguments": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50, "limit_cards": 10 } }
    }));
    let context_pack_text = extract_tool_text(&context_pack);
    assert_eq!(
        context_pack_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let context_cards = context_pack_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("context_pack cards");
    assert!(
        context_cards
            .iter()
            .any(|card| card.get("id").and_then(|v| v.as_str()) == Some(context_card_id)),
        "context_pack must include the newly added think card"
    );

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
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
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } }
    }));

    let export_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
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
fn branchmind_graph_conflicts_and_resolution_smoke() {
    let mut server = Server::start("branchmind_graph_conflicts_and_resolution_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_graph", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let task_id = decompose_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let node_id = "n1";
    let initial_title = "Initial title";
    let apply_initial = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": initial_title } ] } }
    }));
    let apply_initial_text = extract_tool_text(&apply_initial);
    assert_eq!(
        apply_initial_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let base_branch = apply_initial_text
        .get("result")
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("branch")
        .to_string();
    let doc = apply_initial_text
        .get("result")
        .and_then(|v| v.get("doc"))
        .and_then(|v| v.as_str())
        .expect("doc")
        .to_string();

    let derived_branch = format!("{base_branch}/graph_alt");
    let branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_create", "arguments": { "workspace": "ws_graph", "name": derived_branch.clone(), "from": base_branch.clone() } }
    }));
    let branch_create_text = extract_tool_text(&branch_create);
    assert_eq!(
        branch_create_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let base_title = "Base title";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": base_title } ] } }
    }));

    let derived_title = "Derived title";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "branch": derived_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": derived_title } ] } }
    }));

    let diff = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_diff", "arguments": { "workspace": "ws_graph", "from": base_branch.clone(), "to": derived_branch.clone(), "doc": doc.clone(), "limit": 50 } }
    }));
    let diff_text = extract_tool_text(&diff);
    assert_eq!(
        diff_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let changes = diff_text
        .get("result")
        .and_then(|v| v.get("changes"))
        .and_then(|v| v.as_array())
        .expect("diff changes");
    let node_change = changes.iter().find(|c| {
        c.get("kind").and_then(|v| v.as_str()) == Some("node")
            && c.get("id").and_then(|v| v.as_str()) == Some(node_id)
    });
    let node_change = node_change.expect("expected node change for n1");
    let change_title = node_change
        .get("to")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("to.title");
    assert_eq!(change_title, derived_title);

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_merge", "arguments": { "workspace": "ws_graph", "from": derived_branch.clone(), "into": base_branch.clone(), "doc": doc.clone(), "limit": 200 } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflicts_created = merge_text
        .get("result")
        .and_then(|v| v.get("conflicts_created"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(conflicts_created, 1, "expected exactly one conflict");
    let conflict_ids = merge_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert_eq!(conflict_ids.len(), 1, "expected one conflict_id");

    let conflicts_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflicts", "arguments": { "workspace": "ws_graph", "into": base_branch.clone(), "doc": doc.clone(), "limit": 50 } }
    }));
    let conflicts_list_text = extract_tool_text(&conflicts_list);
    assert_eq!(
        conflicts_list_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflicts = conflicts_list_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert_eq!(conflicts.len(), 1, "expected exactly one conflict summary");
    let conflict_id = conflicts[0]
        .get("conflict_id")
        .and_then(|v| v.as_str())
        .expect("conflict_id")
        .to_string();

    let invalid_conflict_show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflict_show", "arguments": { "workspace": "ws_graph", "conflict_id": "CONFLICT-xyz" } }
    }));
    let invalid_conflict_show_text = extract_tool_text(&invalid_conflict_show);
    assert_eq!(
        invalid_conflict_show_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    let invalid_conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflict_resolve", "arguments": { "workspace": "ws_graph", "conflict_id": "CONFLICT-xyz", "resolution": "use_from" } }
    }));
    let invalid_conflict_resolve_text = extract_tool_text(&invalid_conflict_resolve);
    assert_eq!(
        invalid_conflict_resolve_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    let conflict_show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflict_show", "arguments": { "workspace": "ws_graph", "conflict_id": conflict_id.clone() } }
    }));
    let conflict_show_text = extract_tool_text(&conflict_show);
    assert_eq!(
        conflict_show_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict = conflict_show_text
        .get("result")
        .and_then(|v| v.get("conflict"))
        .expect("conflict");
    assert_eq!(conflict.get("kind").and_then(|v| v.as_str()), Some("node"));
    let base_title_shown = conflict
        .get("base")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("base.title");
    assert_eq!(base_title_shown, initial_title);
    let theirs_title_shown = conflict
        .get("theirs")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("theirs.title");
    assert_eq!(theirs_title_shown, derived_title);
    let ours_title_shown = conflict
        .get("ours")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("ours.title");
    assert_eq!(ours_title_shown, base_title);

    let conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflict_resolve", "arguments": { "workspace": "ws_graph", "conflict_id": conflict_id.clone(), "resolution": "use_from" } }
    }));
    let conflict_resolve_text = extract_tool_text(&conflict_resolve);
    assert_eq!(
        conflict_resolve_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let applied = conflict_resolve_text
        .get("result")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        applied,
        "expected applied=true when resolving with use_from"
    );

    let query_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_query", "arguments": { "workspace": "ws_graph", "branch": base_branch.clone(), "doc": doc.clone(), "ids": [node_id], "include_edges": false, "limit": 10 } }
    }));
    let query_base_text = extract_tool_text(&query_base);
    assert_eq!(
        query_base_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_base_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let node = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(node_id))
        .expect("node n1");
    let final_title = node
        .get("title")
        .and_then(|v| v.as_str())
        .expect("node.title");
    assert_eq!(final_title, derived_title);

    let validate_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_validate", "arguments": { "workspace": "ws_graph", "branch": base_branch.clone(), "doc": doc.clone(), "max_errors": 50, "max_chars": 2000 } }
    }));
    let validate_base_text = extract_tool_text(&validate_base);
    assert_eq!(
        validate_base_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let ok = validate_base_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(ok, "expected ok=true after conflict resolution");

    let edge_from = "edge_from";
    let edge_to = "edge_to";
    let apply_edge_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "node_upsert", "id": edge_from, "type": "idea", "title": "Edge From" },
            { "op": "node_upsert", "id": edge_to, "type": "idea", "title": "Edge To" },
            { "op": "edge_upsert", "from": edge_from, "rel": "supports", "to": edge_to }
        ] } }
    }));
    let apply_edge_base_text = extract_tool_text(&apply_edge_base);
    assert_eq!(
        apply_edge_base_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let edge_branch = format!("{base_branch}/edge_alt");
    let edge_branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_create", "arguments": { "workspace": "ws_graph", "name": edge_branch.clone(), "from": base_branch.clone() } }
    }));
    let edge_branch_create_text = extract_tool_text(&edge_branch_create);
    assert_eq!(
        edge_branch_create_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "node_delete", "id": edge_to }
        ] } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_apply", "arguments": { "workspace": "ws_graph", "branch": edge_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "edge_upsert", "from": edge_from, "rel": "supports", "to": edge_to, "meta": { "source": "derived" } }
        ] } }
    }));

    let edge_merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_merge", "arguments": { "workspace": "ws_graph", "from": edge_branch.clone(), "into": base_branch.clone(), "doc": doc.clone(), "limit": 200 } }
    }));
    let edge_merge_text = extract_tool_text(&edge_merge);
    assert_eq!(
        edge_merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let edge_conflicts_created = edge_merge_text
        .get("result")
        .and_then(|v| v.get("conflicts_created"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(edge_conflicts_created, 1, "expected one edge conflict");
    let edge_conflict_ids = edge_merge_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("edge conflict_ids");
    let edge_conflict_id = edge_conflict_ids
        .first()
        .and_then(|v| v.as_str())
        .expect("edge conflict id");

    let edge_conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_conflict_resolve", "arguments": { "workspace": "ws_graph", "conflict_id": edge_conflict_id, "resolution": "use_from" } }
    }));
    let edge_conflict_resolve_text = extract_tool_text(&edge_conflict_resolve);
    assert_eq!(
        edge_conflict_resolve_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let validate_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_validate", "arguments": { "workspace": "ws_graph", "branch": base_branch, "doc": doc, "max_errors": 50, "max_chars": 2000 } }
    }));
    let validate_after_text = extract_tool_text(&validate_after);
    assert_eq!(
        validate_after_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let ok_after = validate_after_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(!ok_after, "expected ok=false for missing edge endpoint");
    let errors = validate_after_text
        .get("result")
        .and_then(|v| v.get("errors"))
        .and_then(|v| v.as_array())
        .expect("errors");
    let has_missing_endpoint = errors
        .iter()
        .any(|e| e.get("code").and_then(|v| v.as_str()) == Some("EDGE_ENDPOINT_MISSING"));
    assert!(
        has_missing_endpoint,
        "expected EDGE_ENDPOINT_MISSING in validation errors"
    );
}

#[test]
fn tasks_graph_projection_smoke() {
    let mut server = Server::start("tasks_graph_projection_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_proj", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_proj", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let step_id = decompose_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let graph_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("graph_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.graph_doc")
        .to_string();

    let task_node = format!("task:{task_id}");
    let step_node = format!("step:{step_id}");
    let query = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_query", "arguments": { "workspace": "ws_graph_proj", "branch": branch, "doc": graph_doc, "ids": [task_node.clone(), step_node.clone()], "include_edges": true, "edges_limit": 10, "limit": 10 } }
    }));
    let query_text = extract_tool_text(&query);
    assert_eq!(
        query_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let task_node_entry = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(task_node.as_str()))
        .expect("task node");
    assert_eq!(
        task_node_entry.get("type").and_then(|v| v.as_str()),
        Some("task")
    );
    let step_node_entry = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(step_node.as_str()))
        .expect("step node");
    assert_eq!(
        step_node_entry.get("type").and_then(|v| v.as_str()),
        Some("step")
    );

    let edges = query_text
        .get("result")
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.as_array())
        .expect("edges");
    let edge = edges.iter().find(|e| {
        e.get("from").and_then(|v| v.as_str()) == Some(task_node.as_str())
            && e.get("rel").and_then(|v| v.as_str()) == Some("contains")
            && e.get("to").and_then(|v| v.as_str()) == Some(step_node.as_str())
    });
    assert!(edge.is_some(), "expected contains edge task -> step");

    let verify = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_verify", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } } } }
    }));
    let verify_text = extract_tool_text(&verify);
    assert_eq!(
        verify_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "step_id": step_id.clone() } }
    }));
    let done_text = extract_tool_text(&done);
    assert_eq!(
        done_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let query_done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_query", "arguments": { "workspace": "ws_graph_proj", "branch": branch.clone(), "doc": graph_doc.clone(), "ids": [step_node.clone()], "include_edges": false, "limit": 10 } }
    }));
    let query_done_text = extract_tool_text(&query_done);
    let done_nodes = query_done_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let done_node = done_nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(step_node.as_str()))
        .expect("step node");
    assert_eq!(
        done_node.get("status").and_then(|v| v.as_str()),
        Some("done")
    );
}

#[test]
fn tasks_close_step_smoke() {
    let mut server = Server::start("tasks_close_step_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_close", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_close", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_close", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let step_id = decompose_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_close_step", "arguments": { "workspace": "ws_close", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } } } }
    }));
    let close_text = extract_tool_text(&close);
    assert_eq!(
        close_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let events = close_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 2);
    assert_eq!(
        events
            .first()
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("step_verified")
    );
    assert_eq!(
        events
            .last()
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("step_done")
    );
}

#[test]
fn branchmind_think_card_and_context_smoke() {
    let mut server = Server::start("branchmind_think_card_and_context_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_think", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let canonical_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();
    let graph_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("graph_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.graph_doc")
        .to_string();

    let template = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_think_template", "arguments": { "workspace": "ws_think", "type": "hypothesis" } }
    }));
    let template_text = extract_tool_text(&template);
    assert_eq!(
        template_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let auto_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 55,
        "method": "tools/call",
        "params": { "name": "branchmind_think_card", "arguments": { "workspace": "ws_think", "target": task_id.clone(), "card": "Quick note" } }
    }));
    let auto_card_text = extract_tool_text(&auto_card);
    assert_eq!(
        auto_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let auto_id = auto_card_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("auto card id");
    assert!(auto_id.starts_with("CARD-"), "auto id must be generated");

    let card_id = "CARD-EXPLICIT-1";
    let title = "Hypothesis";
    let text = "It should improve UX";
    let think_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_think_card", "arguments": { "workspace": "ws_think", "target": task_id.clone(), "card": { "id": card_id, "type": "hypothesis", "title": title, "text": text, "tags": ["UX", "MVP"], "meta": { "why": "smoke" } } } }
    }));
    let think_card_text = extract_tool_text(&think_card);
    assert_eq!(
        think_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        think_card_text
            .get("result")
            .and_then(|v| v.get("card_id"))
            .and_then(|v| v.as_str()),
        Some(card_id)
    );
    let inserted = think_card_text
        .get("result")
        .and_then(|v| v.get("inserted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(inserted, "first think_card call must insert trace entry");

    let show_trace = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws_think", "branch": canonical_branch.clone(), "doc": trace_doc.clone(), "limit": 50 } }
    }));
    let show_trace_text = extract_tool_text(&show_trace);
    let entries = show_trace_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries.iter().any(|e| {
            e.get("kind").and_then(|v| v.as_str()) == Some("note")
                && e.get("format").and_then(|v| v.as_str()) == Some("think_card")
                && e.get("title").and_then(|v| v.as_str()) == Some(title)
                && e.get("meta")
                    .and_then(|v| v.get("card_id"))
                    .and_then(|v| v.as_str())
                    == Some(card_id)
        }),
        "trace must include think_card note entry"
    );

    let query_graph = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_graph_query", "arguments": { "workspace": "ws_think", "branch": canonical_branch.clone(), "doc": graph_doc.clone(), "ids": [card_id], "include_edges": false, "limit": 10 } }
    }));
    let query_graph_text = extract_tool_text(&query_graph);
    assert_eq!(
        query_graph_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_graph_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    assert!(
        nodes.iter().any(|n| {
            n.get("id").and_then(|v| v.as_str()) == Some(card_id)
                && n.get("type").and_then(|v| v.as_str()) == Some("hypothesis")
                && n.get("title").and_then(|v| v.as_str()) == Some(title)
        }),
        "graph must include think card node"
    );

    let think_card_again = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_think_card", "arguments": { "workspace": "ws_think", "target": task_id.clone(), "card": { "id": card_id, "type": "hypothesis", "title": title, "text": text, "tags": ["ux", "mvp"], "meta": { "why": "smoke" } } } }
    }));
    let think_card_again_text = extract_tool_text(&think_card_again);
    assert_eq!(
        think_card_again_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let inserted2 = think_card_again_text
        .get("result")
        .and_then(|v| v.get("inserted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(
        !inserted2,
        "second think_card call must be idempotent (inserted=false)"
    );

    let ctx = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_think_context", "arguments": { "workspace": "ws_think", "branch": canonical_branch, "graph_doc": graph_doc, "limit_cards": 10, "max_chars": 2000 } }
    }));
    let ctx_text = extract_tool_text(&ctx);
    assert_eq!(
        ctx_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards = ctx_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some(card_id)),
        "think_context must include the card"
    );
}

#[test]
fn branchmind_vcs_smoke() {
    let mut server = Server::start("branchmind_vcs_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "branchmind_init", "arguments": { "workspace": "ws_vcs" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let commit1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branchmind_commit", "arguments": { "workspace": "ws_vcs", "artifact": "artifact-1", "message": "m1" } }
    }));
    let commit1_text = extract_tool_text(&commit1);
    let seq1 = commit1_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("commit seq1");

    let commit2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "branchmind_commit", "arguments": { "workspace": "ws_vcs", "artifact": "artifact-2", "message": "m2" } }
    }));
    let commit2_text = extract_tool_text(&commit2);
    let seq2 = commit2_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("commit seq2");
    assert!(seq2 > seq1, "commit seq must advance");

    let log = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_log", "arguments": { "workspace": "ws_vcs", "limit": 10 } }
    }));
    let log_text = extract_tool_text(&log);
    let commits = log_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .expect("commits");
    assert!(commits.len() >= 2, "log must include commits");

    let tag = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_tag_create", "arguments": { "workspace": "ws_vcs", "name": "v1", "from": seq1.to_string() } }
    }));
    let tag_text = extract_tool_text(&tag);
    assert_eq!(
        tag_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let tags = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_tag_list", "arguments": { "workspace": "ws_vcs" } }
    }));
    let tags_text = extract_tool_text(&tags);
    let tags_list = tags_text
        .get("result")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_array())
        .expect("tags list");
    assert!(
        tags_list
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("v1")),
        "tag v1 must exist"
    );

    let reset = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_reset", "arguments": { "workspace": "ws_vcs", "ref": seq1.to_string() } }
    }));
    let reset_text = extract_tool_text(&reset);
    assert_eq!(
        reset_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let log_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_log", "arguments": { "workspace": "ws_vcs", "limit": 10 } }
    }));
    let log_after_text = extract_tool_text(&log_after);
    let commits_after = log_after_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .expect("commits after reset");
    assert!(
        !commits_after
            .iter()
            .any(|c| c.get("seq").and_then(|v| v.as_i64()) == Some(seq2)),
        "reset must drop later commits from log view"
    );

    let reflog = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_reflog", "arguments": { "workspace": "ws_vcs", "limit": 10 } }
    }));
    let reflog_text = extract_tool_text(&reflog);
    let reflog_entries = reflog_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("reflog entries");
    assert!(!reflog_entries.is_empty(), "reflog must have entries");

    let docs = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "branchmind_docs_list", "arguments": { "workspace": "ws_vcs" } }
    }));
    let docs_text = extract_tool_text(&docs);
    let docs_list = docs_text
        .get("result")
        .and_then(|v| v.get("docs"))
        .and_then(|v| v.as_array())
        .expect("docs list");
    assert!(
        docs_list
            .iter()
            .any(|d| d.get("doc").and_then(|v| v.as_str()) == Some("notes")),
        "notes doc must exist"
    );

    let branch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_create", "arguments": { "workspace": "ws_vcs", "name": "topic" } }
    }));
    let branch_text = extract_tool_text(&branch);
    assert_eq!(
        branch_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let rename = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_rename", "arguments": { "workspace": "ws_vcs", "old": "topic", "new": "topic-renamed" } }
    }));
    let rename_text = extract_tool_text(&rename);
    assert_eq!(
        rename_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let delete = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "branchmind_branch_delete", "arguments": { "workspace": "ws_vcs", "name": "topic-renamed" } }
    }));
    let delete_text = extract_tool_text(&delete);
    let deleted = delete_text
        .get("result")
        .and_then(|v| v.get("deleted"))
        .and_then(|v| v.as_bool())
        .expect("delete result");
    assert!(deleted, "branch must be deleted");
}

#[test]
fn branchmind_think_wrappers_smoke() {
    let mut server = Server::start("branchmind_think_wrappers_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "branchmind_init", "arguments": { "workspace": "ws_think_wrap" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_note", "arguments": { "workspace": "ws_think_wrap", "card": "Quick note" } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let decision = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_decision", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Decision", "text": "Proceed" } } }
    }));
    let decision_text = extract_tool_text(&decision);
    assert_eq!(
        decision_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let evidence = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_evidence", "arguments": { "workspace": "ws_think_wrap", "card": "Evidence collected" } }
    }));
    let evidence_text = extract_tool_text(&evidence);
    assert_eq!(
        evidence_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let frame = server.request(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_frame", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Frame", "text": "Context" } } }
    }));
    let frame_text = extract_tool_text(&frame);
    assert_eq!(
        frame_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let update = server.request(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_update", "arguments": { "workspace": "ws_think_wrap", "card": "Progress update" } }
    }));
    let update_text = extract_tool_text(&update);
    assert_eq!(
        update_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let hypo1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_hypothesis", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } }
    }));
    let hypo1_text = extract_tool_text(&hypo1);
    let hypo1_id = hypo1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo1 id")
        .to_string();

    let hypo2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_hypothesis", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } }
    }));
    let hypo2_text = extract_tool_text(&hypo2);
    let hypo2_id = hypo2_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo2 id")
        .to_string();

    let question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_question", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Question", "text": "Why?" } } }
    }));
    let question_text = extract_tool_text(&question);
    let question_id = question_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("question id")
        .to_string();

    let test_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_think_add_test", "arguments": { "workspace": "ws_think_wrap", "card": "Test it" } }
    }));
    let test_text = extract_tool_text(&test_card);
    assert_eq!(
        test_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let link = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_think_link", "arguments": { "workspace": "ws_think_wrap", "from": question_id.clone(), "rel": "supports", "to": hypo1_id.clone() } }
    }));
    let link_text = extract_tool_text(&link);
    assert_eq!(
        link_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let set_status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "branchmind_think_set_status", "arguments": { "workspace": "ws_think_wrap", "status": "blocked", "targets": [hypo1_id.clone()] } }
    }));
    let set_status_text = extract_tool_text(&set_status);
    assert_eq!(
        set_status_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pin = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "branchmind_think_pin", "arguments": { "workspace": "ws_think_wrap", "targets": [hypo1_id.clone()] } }
    }));
    let pin_text = extract_tool_text(&pin);
    assert_eq!(
        pin_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pins = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "branchmind_think_pins", "arguments": { "workspace": "ws_think_wrap", "limit": 10 } }
    }));
    let pins_text = extract_tool_text(&pins);
    let pins_list = pins_text
        .get("result")
        .and_then(|v| v.get("pins"))
        .and_then(|v| v.as_array())
        .expect("pins");
    assert!(
        pins_list
            .iter()
            .any(|p| p.get("id").and_then(|v| v.as_str()) == Some(hypo1_id.as_str())),
        "pinned card must be listed"
    );

    let query = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "branchmind_think_query", "arguments": { "workspace": "ws_think_wrap", "types": "hypothesis", "limit": 10 } }
    }));
    let query_text = extract_tool_text(&query);
    assert_eq!(
        query_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let query_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 111,
        "method": "tools/call",
        "params": { "name": "branchmind_think_query", "arguments": { "workspace": "ws_think_wrap", "types": "hypothesis", "limit": 10, "max_chars": 200 } }
    }));
    let query_budgeted_text = extract_tool_text(&query_budgeted);
    let budget = query_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let used = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let max = budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        used <= max,
        "budget.used_chars must be <= max_chars for think_query"
    );

    let frontier = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "branchmind_think_frontier", "arguments": { "workspace": "ws_think_wrap" } }
    }));
    let frontier_text = extract_tool_text(&frontier);
    assert_eq!(
        frontier_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let frontier_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 121,
        "method": "tools/call",
        "params": { "name": "branchmind_think_frontier", "arguments": { "workspace": "ws_think_wrap", "max_chars": 200 } }
    }));
    let frontier_budgeted_text = extract_tool_text(&frontier_budgeted);
    let frontier_budget = frontier_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let frontier_used = frontier_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let frontier_max = frontier_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        frontier_used <= frontier_max,
        "budget.used_chars must be <= max_chars for think_frontier"
    );
    let frontier_result = frontier_budgeted_text
        .get("result")
        .expect("frontier result");
    assert!(
        frontier_result.get("frontier").is_some() || frontier_result.get("signal").is_some(),
        "think_frontier should return minimal frontier or signal under tiny max_chars"
    );

    let next = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "branchmind_think_next", "arguments": { "workspace": "ws_think_wrap" } }
    }));
    let next_text = extract_tool_text(&next);
    assert_eq!(
        next_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let next_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 131,
        "method": "tools/call",
        "params": { "name": "branchmind_think_next", "arguments": { "workspace": "ws_think_wrap", "max_chars": 120 } }
    }));
    let next_budgeted_text = extract_tool_text(&next_budgeted);
    let next_budget = next_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let next_used = next_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let next_max = next_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        next_used <= next_max,
        "budget.used_chars must be <= max_chars for think_next"
    );
    let next_result = next_budgeted_text.get("result").expect("next result");
    assert!(
        next_result.get("candidate").is_some() || next_result.get("signal").is_some(),
        "think_next should return minimal candidate or signal under tiny max_chars"
    );

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "branchmind_think_pack", "arguments": { "workspace": "ws_think_wrap", "limit_candidates": 10 } }
    }));
    let pack_text = extract_tool_text(&pack);
    assert_eq!(
        pack_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pack_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 114,
        "method": "tools/call",
        "params": { "name": "branchmind_think_pack", "arguments": { "workspace": "ws_think_wrap", "limit_candidates": 10, "max_chars": 400 } }
    }));
    let pack_budget_text = extract_tool_text(&pack_budget);
    let pack_budget_obj = pack_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let pack_used = pack_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let pack_max = pack_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        pack_used <= pack_max,
        "think_pack budget must not exceed max_chars"
    );
    let pack_candidates_len = pack_budget_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let pack_stats_cards = pack_budget_text
        .get("result")
        .and_then(|v| v.get("stats"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        pack_stats_cards as usize >= pack_candidates_len,
        "think_pack stats.cards must be >= candidates length"
    );

    let context_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 115,
        "method": "tools/call",
        "params": { "name": "branchmind_think_context", "arguments": { "workspace": "ws_think_wrap", "limit_cards": 10, "max_chars": 400 } }
    }));
    let context_budget_text = extract_tool_text(&context_budget);
    let context_budget_obj = context_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let context_used = context_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let context_max = context_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        context_used <= context_max,
        "think_context budget must not exceed max_chars"
    );
    let context_cards_len = context_budget_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let context_stats_cards = context_budget_text
        .get("result")
        .and_then(|v| v.get("stats"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        context_stats_cards as usize, context_cards_len,
        "think_context stats.cards must match cards length"
    );

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": { "name": "branchmind_think_nominal_merge", "arguments": { "workspace": "ws_think_wrap", "candidate_ids": [hypo1_id.clone(), hypo2_id.clone()] } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let playbook = server.request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": { "name": "branchmind_think_playbook", "arguments": { "workspace": "ws_think_wrap", "name": "default" } }
    }));
    let playbook_text = extract_tool_text(&playbook);
    assert_eq!(
        playbook_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let subgoal_open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": { "name": "branchmind_think_subgoal_open", "arguments": { "workspace": "ws_think_wrap", "question_id": question_id.clone() } }
    }));
    let subgoal_open_text = extract_tool_text(&subgoal_open);
    let subgoal_id = subgoal_open_text
        .get("result")
        .and_then(|v| v.get("subgoal_id"))
        .and_then(|v| v.as_str())
        .expect("subgoal id")
        .to_string();

    let subgoal_close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": { "name": "branchmind_think_subgoal_close", "arguments": { "workspace": "ws_think_wrap", "subgoal_id": subgoal_id, "return_card": "done" } }
    }));
    let subgoal_close_text = extract_tool_text(&subgoal_close);
    assert_eq!(
        subgoal_close_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "tools/call",
        "params": { "name": "branchmind_think_watch", "arguments": { "workspace": "ws_think_wrap", "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    assert_eq!(
        watch_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let watch_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 120,
        "method": "tools/call",
        "params": { "name": "branchmind_think_watch", "arguments": { "workspace": "ws_think_wrap", "limit_candidates": 10, "trace_limit_steps": 20, "max_chars": 400 } }
    }));
    let watch_budget_text = extract_tool_text(&watch_budget);
    let watch_budget_obj = watch_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let watch_used = watch_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let watch_max = watch_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        watch_used <= watch_max,
        "think_watch budget must not exceed max_chars"
    );
    let watch_entries_len = watch_budget_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let watch_count = watch_budget_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("pagination"))
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        watch_used <= watch_max,
        "think_watch budget must not exceed max_chars"
    );
    assert_eq!(
        watch_count as usize, watch_entries_len,
        "think_watch pagination.count must match entries length"
    );

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "branchmind_think_lint", "arguments": { "workspace": "ws_think_wrap" } }
    }));
    let lint_text = extract_tool_text(&lint);
    assert_eq!(
        lint_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn branchmind_think_pipeline_smoke() {
    let mut server = Server::start("branchmind_think_pipeline_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think_pipe", "kind": "plan", "title": "Plan Pipe" } }
    }));
    let plan_id = extract_tool_text(&created_plan)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think_pipe", "kind": "task", "parent": plan_id, "title": "Task Pipe" } }
    }));
    let task_id = extract_tool_text(&created_task)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let pipeline = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "branchmind_think_pipeline",
            "arguments": {
                "workspace": "ws_think_pipe",
                "target": task_id,
                "frame": "Frame",
                "hypothesis": "Hypothesis",
                "test": "Test",
                "evidence": "Evidence",
                "decision": "Decision"
            }
        }
    }));
    let pipeline_text = extract_tool_text(&pipeline);
    let cards = pipeline_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert_eq!(cards.len(), 5);
    let decision_note = pipeline_text
        .get("result")
        .and_then(|v| v.get("decision_note"))
        .expect("decision_note");
    assert!(decision_note.get("card_id").is_some());
}

#[test]
fn branchmind_trace_smoke() {
    let mut server = Server::start("branchmind_trace_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "branchmind_init", "arguments": { "workspace": "ws_trace" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_trace", "kind": "plan", "title": "Trace Plan" } }
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
        "id": 21,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_trace", "kind": "task", "parent": plan_id, "title": "Trace Task" } }
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
        "id": 22,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_trace", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let target_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let target_trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();

    let target_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "branchmind_trace_step", "arguments": { "workspace": "ws_trace", "target": task_id, "step": "Target step" } }
    }));
    let target_step_text = extract_tool_text(&target_step);
    assert_eq!(
        target_step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(target_branch.as_str())
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some(target_trace_doc.as_str())
    );

    let step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branchmind_trace_step", "arguments": { "workspace": "ws_trace", "step": "Step 1", "message": "m1" } }
    }));
    let step_text = extract_tool_text(&step);
    let seq1 = step_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace step seq");

    let seq_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "branchmind_trace_sequential_step",
            "arguments": {
                "workspace": "ws_trace",
                "thought": "Thought 1",
                "thoughtNumber": 1,
                "totalThoughts": 2,
                "nextThoughtNeeded": true
            }
        }
    }));
    let seq_text = extract_tool_text(&seq_step);
    let seq2 = seq_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace sequential seq");
    assert!(seq2 > seq1, "sequential step must advance seq");

    let hydrate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "branchmind_trace_hydrate", "arguments": { "workspace": "ws_trace", "limit_steps": 10 } }
    }));
    let hydrate_text = extract_tool_text(&hydrate);
    let entries = hydrate_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace entries");
    assert!(entries.len() >= 2, "trace hydrate must return entries");

    let validate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_trace_validate", "arguments": { "workspace": "ws_trace" } }
    }));
    let validate_text = extract_tool_text(&validate);
    let ok = validate_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .expect("trace validate ok");
    assert!(ok, "trace validate must be ok");
}

#[test]
fn tasks_note_is_mirrored_into_reasoning_notes() {
    let mut server = Server::start("tasks_note_mirrored_into_reasoning_notes");

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

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws1", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let step_path = decompose_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .expect("step path")
        .to_string();

    let note_content = "progress note via tasks_note";
    let noted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_note", "arguments": { "workspace": "ws1", "task": task_id.clone(), "path": step_path, "note": note_content } }
    }));
    let noted_text = extract_tool_text(&noted);
    assert_eq!(
        noted_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let show_notes = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "branchmind_show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "notes", "limit": 50 } }
    }));
    let show_notes_text = extract_tool_text(&show_notes);
    let entries = show_notes_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "expected tasks_note content to be mirrored into reasoning notes"
    );

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branchmind_export", "arguments": { "workspace": "ws1", "target": task_id, "notes_limit": 50, "trace_limit": 50 } }
    }));
    let export_text = extract_tool_text(&export);
    let export_notes_entries = export_text
        .get("result")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(
        export_notes_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "expected branchmind_export to include mirrored tasks_note content"
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
        created_task_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

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

    let context_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 56,
        "method": "tools/call",
        "params": { "name": "tasks_context_pack", "arguments": { "workspace": "ws1", "task": task_id.clone(), "delta_limit": 50, "max_chars": 400 } }
    }));
    let context_pack_text = extract_tool_text(&context_pack);
    let pack_budget = context_pack_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let pack_used = pack_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let pack_max = pack_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        pack_used <= pack_max,
        "tasks_context_pack budget must not exceed max_chars"
    );

    let context_limited = server.request(json!({
        "jsonrpc": "2.0",
        "id": 55,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws1", "max_chars": 10 } }
    }));
    let ctx_limited_text = extract_tool_text(&context_limited);
    let limited_budget = ctx_limited_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    assert_eq!(
        limited_budget.get("truncated").and_then(|v| v.as_bool()),
        Some(true)
    );
    let limited_result = ctx_limited_text.get("result").expect("result");
    if let Some(plans) = limited_result.get("plans").and_then(|v| v.as_array()) {
        assert!(plans.is_empty());
    }
    if let Some(tasks) = limited_result.get("tasks").and_then(|v| v.as_array()) {
        assert!(tasks.is_empty());
    }

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

    let delta_limited = server.request(json!({
        "jsonrpc": "2.0",
        "id": 66,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1", "max_chars": 10 } }
    }));
    let delta_limited_text = extract_tool_text(&delta_limited);
    let limited_budget = delta_limited_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    assert_eq!(
        limited_budget.get("truncated").and_then(|v| v.as_bool()),
        Some(true)
    );
    if let Some(events) = delta_limited_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
    {
        assert!(events.is_empty());
    }
}

#[test]
fn tasks_bootstrap_and_resume_pack_smoke() {
    let mut server = Server::start("tasks_bootstrap_and_resume_pack");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Bootstrap",
                "task_title": "Task Bootstrap",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] },
                    { "title": "S2", "success_criteria": ["c2"], "tests": ["t2"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let steps = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    assert_eq!(steps.len(), 2);

    let resume_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_resume_pack", "arguments": { "workspace": "ws1", "task": task_id.clone(), "events_limit": 10, "max_chars": 2000 } }
    }));
    let resume_pack_text = extract_tool_text(&resume_pack);
    assert_eq!(
        resume_pack_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert!(
        resume_pack_text
            .get("result")
            .and_then(|v| v.get("radar"))
            .is_some()
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws1" } }
    }));
    let focus_text = extract_tool_text(&focus);
    assert_eq!(
        focus_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
}

#[test]
fn tasks_resume_super_read_only_smoke() {
    let mut server = Server::start("tasks_resume_super_read_only_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Super",
                "task_title": "Task Super",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let plan_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws1", "task": plan_id } }
    }));

    let resume_super = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": { "workspace": "ws1", "task": task_id.clone(), "read_only": true, "max_chars": 4000 } }
    }));
    let resume_text = extract_tool_text(&resume_super);
    assert!(
        resume_text
            .get("result")
            .and_then(|v| v.get("memory"))
            .is_some()
    );
    assert!(
        resume_text
            .get("result")
            .and_then(|v| v.get("degradation"))
            .is_some()
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws1" } }
    }));
    let focus_text = extract_tool_text(&focus);
    assert_eq!(
        focus_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(plan_id.as_str())
    );
}

#[test]
fn tasks_macro_flow_smoke() {
    let mut server = Server::start("tasks_macro_flow_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Macro",
                "task_title": "Task Macro",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    let start_text = extract_tool_text(&start);
    let task_id = start_text
        .get("result")
        .and_then(|v| v.get("task_id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step_path = start_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .expect("step path")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": {
                "workspace": "ws1",
                "task": task_id,
                "path": step_path,
                "checkpoints": {
                    "criteria": { "confirmed": true },
                    "tests": { "confirmed": true },
                    "security": { "confirmed": true },
                    "perf": { "confirmed": true },
                    "docs": { "confirmed": true }
                },
                "resume_max_chars": 4000
            }
        }
    }));
    let close_text = extract_tool_text(&close);
    assert!(
        close_text
            .get("result")
            .and_then(|v| v.get("resume"))
            .is_some()
    );

    let finish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_finish",
            "arguments": {
                "workspace": "ws1",
                "task": close_text.get("result").and_then(|v| v.get("task")).and_then(|v| v.as_str()).unwrap()
            }
        }
    }));
    let finish_text = extract_tool_text(&finish);
    assert!(
        finish_text
            .get("result")
            .and_then(|v| v.get("handoff"))
            .is_some()
    );
}

#[test]
fn tasks_lint_context_health_smoke() {
    let mut server = Server::start("tasks_lint_context_health_smoke");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Lint",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id");

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);
    assert!(
        lint_text
            .get("result")
            .and_then(|v| v.get("context_health"))
            .is_some()
    );
}

#[test]
fn tasks_bootstrap_with_think_pipeline() {
    let mut server = Server::start("tasks_bootstrap_with_think_pipeline");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Think",
                "task_title": "Task Think",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "think": {
                    "frame": "Bootstrap frame",
                    "decision": "Bootstrap decision"
                }
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    assert_eq!(
        bootstrap_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let pipeline = bootstrap_text
        .get("result")
        .and_then(|v| v.get("think_pipeline"))
        .expect("think_pipeline");
    let cards = pipeline
        .get("cards")
        .and_then(|v| v.as_array())
        .expect("think_pipeline.cards");
    assert!(cards.len() >= 2);
    assert!(pipeline.get("decision_note").is_some());
}

#[test]
fn tasks_define_normalizes_blockers() {
    let mut server = Server::start("tasks_define_normalizes_blockers");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Normalize",
                "task_title": "Task Normalize",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step_path = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .expect("step path")
        .to_string();

    let defined = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_define", "arguments": { "workspace": "ws1", "task": task_id.clone(), "path": step_path, "blockers": ["None"] } }
    }));
    let defined_text = extract_tool_text(&defined);
    assert_eq!(
        defined_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));
    let resume_text = extract_tool_text(&resume);
    let steps = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    let blockers = steps
        .iter()
        .find(|s| s.get("step_id").is_some())
        .and_then(|s| s.get("blockers"))
        .and_then(|v| v.as_array())
        .expect("blockers");
    assert!(blockers.is_empty());
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
    assert_eq!(
        radar_from_focus_text
            .get("result")
            .and_then(|v| v.get("budget"))
            .and_then(|v| v.get("max_chars"))
            .and_then(|v| v.as_u64()),
        Some(10)
    );

    let radar_full = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1", "max_chars": 400 } }
    }));
    let radar_full_text = extract_tool_text(&radar_full);
    let expected_branch = format!("task/{task_id}");
    assert_eq!(
        radar_full_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert_eq!(
        radar_full_text
            .get("result")
            .and_then(|v| v.get("reasoning_ref"))
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(expected_branch.as_str())
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
