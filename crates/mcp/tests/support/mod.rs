#![forbid(unsafe_code)]
#![allow(dead_code)]

use serde_json::Value;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(crate) struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    storage_dir: PathBuf,
    cleanup_storage: bool,
}

impl Server {
    pub(crate) fn start(test_name: &str) -> Self {
        Self::start_with_args(test_name, &[])
    }

    pub(crate) fn start_with_args(test_name: &str, extra_args: &[&str]) -> Self {
        let storage_dir = temp_dir(test_name);
        Self::start_with_storage_dir(storage_dir, extra_args, true)
    }

    pub(crate) fn start_with_storage_dir(
        storage_dir: PathBuf,
        extra_args: &[&str],
        cleanup_storage: bool,
    ) -> Self {
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let has_toolset = extra_args.iter().any(|arg| arg.trim() == "--toolset");
        let default_toolset: &[&str] = if has_toolset {
            &[]
        } else {
            &["--toolset", "full"]
        };
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
            .arg(&storage_dir)
            .args(default_toolset)
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
            storage_dir,
            cleanup_storage,
        }
    }

    pub(crate) fn send(&mut self, req: Value) {
        writeln!(self.stdin, "{req}").expect("write request");
        self.stdin.flush().expect("flush request");
    }

    pub(crate) fn recv(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(!line.trim().is_empty(), "empty response line");
        serde_json::from_str(&line).expect("parse response json")
    }

    pub(crate) fn request(&mut self, req: Value) -> Value {
        let mut req = req;
        rewrite_tool_call(&mut req);
        self.send(req);
        self.recv()
    }

    pub(crate) fn request_raw(&mut self, req: Value) -> Value {
        self.send(req);
        self.recv()
    }

    pub(crate) fn initialize_default(&mut self) {
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

    pub(crate) fn start_initialized(test_name: &str) -> Self {
        let mut server = Self::start(test_name);
        server.initialize_default();
        server
    }

    pub(crate) fn start_initialized_with_args(test_name: &str, extra_args: &[&str]) -> Self {
        let mut server = Self::start_with_args(test_name, extra_args);
        server.initialize_default();
        server
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.cleanup_storage {
            let _ = std::fs::remove_dir_all(&self.storage_dir);
        }
    }
}

pub(crate) fn rewrite_tool_call(req: &mut Value) {
    let Some(obj) = req.as_object_mut() else {
        return;
    };
    if obj.get("method").and_then(|v| v.as_str()) != Some("tools/call") {
        return;
    }
    let Some(params) = obj.get_mut("params").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let raw_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return,
    };
    if raw_name.starts_with("bm.") {
        return;
    }
    let name = normalize_legacy_tool_name(&raw_name);
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if name == "status" {
        params.insert("name".to_string(), json!("status"));
        params.insert("arguments".to_string(), args);
        return;
    }
    if name == "open" {
        params.insert("name".to_string(), json!("open"));
        params.insert("arguments".to_string(), args);
        return;
    }

    if let Some((portal, cmd)) = legacy_tool_to_cmd(&name) {
        let mut envelope = serde_json::Map::new();
        if let Some(ws) = args.get("workspace").cloned() {
            envelope.insert("workspace".to_string(), ws);
        }
        envelope.insert("op".to_string(), json!("call"));
        envelope.insert("cmd".to_string(), json!(cmd));
        envelope.insert("args".to_string(), args);
        params.insert("name".to_string(), json!(portal));
        params.insert("arguments".to_string(), Value::Object(envelope));
    }
}

fn normalize_legacy_tool_name(raw: &str) -> String {
    let mut name = raw.trim();
    if let Some((_, suffix)) = name.rsplit_once('/') {
        name = suffix;
    }
    if let Some((prefix, suffix)) = name.split_once('.')
        && prefix == "branchmind"
    {
        name = suffix;
    }
    name.trim().to_string()
}

fn legacy_tool_to_cmd(name: &str) -> Option<(&'static str, String)> {
    if name == "workspace_use" {
        return Some(("workspace", "workspace.use".to_string()));
    }
    if name == "workspace_reset" {
        return Some(("workspace", "workspace.reset".to_string()));
    }

    if matches!(name, "init" | "help" | "skill" | "diagnostics" | "storage") {
        return Some(("system", format!("system.{name}")));
    }

    if name == "context_pack" {
        return Some(("think", "think.context.pack".to_string()));
    }
    if name == "context_pack_export" {
        return Some(("think", "think.context.pack.export".to_string()));
    }

    if name == "docs_list" {
        return Some(("docs", "docs.list".to_string()));
    }
    if matches!(name, "show" | "diff" | "merge") {
        return Some(("docs", format!("docs.{name}")));
    }
    if name.starts_with("transcripts_") {
        let suffix = name.trim_start_matches("transcripts_");
        return Some(("docs", format!("docs.transcripts.{}", dotted(suffix))));
    }
    if name == "export" {
        return Some(("docs", "docs.export".to_string()));
    }

    if name == "tasks_runner_heartbeat" {
        return Some(("jobs", "jobs.runner.heartbeat".to_string()));
    }
    if let Some(suffix) = name.strip_prefix("tasks_jobs_") {
        return Some(("jobs", format!("jobs.{}", dotted(suffix))));
    }

    if let Some(suffix) = name.strip_prefix("tasks_") {
        let cmd = match suffix {
            "create" => "tasks.plan.create".to_string(),
            "decompose" => "tasks.plan.decompose".to_string(),
            "evidence_capture" => "tasks.evidence.capture".to_string(),
            "close_step" => "tasks.step.close".to_string(),
            _ => format!("tasks.{}", dotted(suffix)),
        };
        return Some(("tasks", cmd));
    }

    if let Some(suffix) = name.strip_prefix("graph_") {
        return Some(("graph", format!("graph.{}", dotted(suffix))));
    }

    if let Some(suffix) = name.strip_prefix("branch_") {
        return Some(("vcs", format!("vcs.branch.{}", dotted(suffix))));
    }
    if let Some(suffix) = name.strip_prefix("tag_") {
        return Some(("vcs", format!("vcs.tag.{}", dotted(suffix))));
    }
    if name == "notes_commit" {
        return Some(("vcs", "vcs.notes.commit".to_string()));
    }
    if matches!(name, "checkout" | "commit" | "log" | "reflog" | "reset") {
        return Some(("vcs", format!("vcs.{name}")));
    }

    if name == "knowledge_list" {
        return Some(("think", "think.knowledge.query".to_string()));
    }
    if name == "think_lint" {
        return Some(("think", "think.knowledge.lint".to_string()));
    }
    if name == "think_template" {
        return Some(("think", "think.reasoning.seed".to_string()));
    }
    if name == "think_pipeline" {
        return Some(("think", "think.reasoning.pipeline".to_string()));
    }
    if name == "macro_branch_note" {
        return Some(("think", "think.idea.branch.create".to_string()));
    }
    if name == "macro_anchor_note" {
        return Some(("think", "think.macro.anchor.note".to_string()));
    }
    if let Some(suffix) = name.strip_prefix("anchors_") {
        return Some(("think", format!("think.anchor.{}", dotted(suffix))));
    }
    if let Some(suffix) = name.strip_prefix("anchor_") {
        return Some(("think", format!("think.anchor.{}", dotted(suffix))));
    }
    if let Some(suffix) = name.strip_prefix("think_") {
        return Some(("think", format!("think.{}", dotted(suffix))));
    }

    if let Some(suffix) = name.strip_prefix("trace_") {
        return Some(("think", format!("think.trace.{}", dotted(suffix))));
    }
    if let Some(suffix) = name.strip_prefix("context_pack_") {
        return Some(("think", format!("think.context.pack.{}", dotted(suffix))));
    }
    if name == "context_pack" {
        return Some(("think", "think.context.pack".to_string()));
    }

    None
}

fn dotted(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', ".")
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

pub(crate) fn extract_tool_text(resp: &Value) -> Value {
    let text = resp
        .get("result")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .expect("result.content[0].text");
    if let Ok(parsed) = serde_json::from_str(text) {
        return parsed;
    }
    Value::String(text.to_string())
}

pub(crate) fn extract_tool_text_str(resp: &Value) -> String {
    resp.get("result")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .expect("result.content[0].text")
        .to_string()
}

pub(crate) fn assert_json_rpc_error(resp: &Value, expected_code: i64) {
    let code = resp
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_i64())
        .expect("error.code");
    assert_eq!(code, expected_code);
}

pub(crate) fn parse_open_command_line(line: &str) -> serde_json::Map<String, Value> {
    let line = line.trim();
    assert!(
        line == "open" || line.starts_with("open "),
        "expected an open command line, got: {line}"
    );
    let mut parts = line.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    assert_eq!(cmd, "open", "expected open command");

    let mut args = serde_json::Map::new();
    for part in parts {
        let Some((key, raw)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let raw = raw.trim();
        if key.is_empty() || raw.is_empty() {
            continue;
        }

        let value = if raw.starts_with('"') || raw.starts_with('[') || raw.starts_with('{') {
            serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.to_string()))
        } else if raw == "true" || raw == "false" {
            Value::Bool(raw == "true")
        } else if let Ok(n) = raw.parse::<i64>() {
            Value::Number(serde_json::Number::from(n))
        } else {
            Value::String(raw.to_string())
        };

        args.insert(key.to_string(), value);
    }

    args
}

pub(crate) fn parse_state_ref_id(state_line: &str) -> Option<String> {
    let idx = state_line.find("ref=")?;
    let after = &state_line[idx + "ref=".len()..];
    let id = after
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('|')
        .trim();
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}
