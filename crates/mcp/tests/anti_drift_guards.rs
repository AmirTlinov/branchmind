#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use support::*;

struct RawServer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl RawServer {
    fn start_with(storage_dir: &std::path::Path, extra_args: &[&str]) -> Self {
        let has_viewer_flag = extra_args
            .iter()
            .any(|arg| matches!(arg.trim(), "--viewer" | "--no-viewer"));
        let default_viewer: &[&str] = if has_viewer_flag {
            &[]
        } else {
            &["--no-viewer"]
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--storage-dir")
            .arg(storage_dir)
            .args(["--toolset", "full"])
            .args(default_viewer)
            .args(extra_args)
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
        }
    }

    fn initialize(&mut self) {
        let _ = self.request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
        }));
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
    }

    fn send(&mut self, req: serde_json::Value) {
        writeln!(self.stdin, "{req}").expect("write request");
        self.stdin.flush().expect("flush request");
    }

    fn recv(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(!line.trim().is_empty(), "empty response line");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn request(&mut self, req: serde_json::Value) -> serde_json::Value {
        self.send(req);
        self.recv()
    }
}

impl Drop for RawServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn workspace_lock_rejects_explicit_workspace_mismatch() {
    let mut server = Server::start_initialized_with_args(
        "workspace_lock_rejects_explicit_workspace_mismatch",
        &["--workspace", "ws_default", "--workspace-lock"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_other", "kind": "plan", "title": "Plan" } }
    }));
    let text = extract_tool_text(&resp);

    assert_eq!(
        resp.get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("WORKSPACE_LOCKED")
    );
}

#[test]
fn project_guard_mismatch_is_typed_error() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let pid = std::process::id();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let storage_dir = std::env::temp_dir().join(format!("bm_project_guard_{pid}_{nonce}"));
    std::fs::create_dir_all(&storage_dir).expect("create temp dir");

    {
        let mut server = RawServer::start_with(&storage_dir, &["--project-guard", "guard-a"]);
        server.initialize();
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "tasks_create", "arguments": { "workspace": "ws_guarded", "kind": "plan", "title": "Plan A" } }
        }));
    }

    {
        let mut server = RawServer::start_with(&storage_dir, &["--project-guard", "guard-b"]);
        server.initialize();
        let resp = server.request(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "tasks_create", "arguments": { "workspace": "ws_guarded", "kind": "plan", "title": "Plan B" } }
        }));

        let text = extract_tool_text(&resp);
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            text.get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str()),
            Some("PROJECT_GUARD_MISMATCH")
        );
    }

    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[test]
fn project_guard_defaults_when_not_explicit() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let pid = std::process::id();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let storage_dir = std::env::temp_dir().join(format!("bm_project_guard_default_{pid}_{nonce}"));
    std::fs::create_dir_all(&storage_dir).expect("create temp dir");

    {
        let mut server = RawServer::start_with(&storage_dir, &["--project-guard", "guard-a"]);
        server.initialize();
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "tasks_create", "arguments": { "workspace": "ws_guarded", "kind": "plan", "title": "Plan A" } }
        }));
    }

    {
        let mut server = RawServer::start_with(&storage_dir, &[]);
        server.initialize();
        let resp = server.request(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "tasks_create", "arguments": { "workspace": "ws_guarded", "kind": "plan", "title": "Plan B" } }
        }));

        let text = extract_tool_text(&resp);
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            text.get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str()),
            Some("PROJECT_GUARD_MISMATCH")
        );
    }

    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[test]
fn advertised_portal_toolsets_stay_minimal() {
    let mut core = Server::start_initialized_with_args(
        "advertised_portal_toolsets_stay_minimal_core",
        &["--toolset", "core"],
    );
    let core_tools =
        core.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let core_names = core_tools
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools")
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        core_names,
        vec!["status", "tasks_macro_start", "tasks_snapshot"],
        "core toolset must stay the 3-tool golden path"
    );

    let mut daily = Server::start_initialized_with_args(
        "advertised_portal_toolsets_stay_minimal_daily",
        &["--toolset", "daily"],
    );
    let daily_tools =
        daily.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let daily_names = daily_tools
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools")
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        daily_names,
        vec![
            "status",
            "tasks_macro_start",
            "tasks_snapshot",
            "open",
            "skill",
            "tasks_jobs_radar",
            "tasks_lint",
            "tasks_macro_close_step",
            "tasks_macro_delegate",
            "think_card",
            "think_playbook",
        ],
        "daily toolset must stay the small portal set"
    );
}
