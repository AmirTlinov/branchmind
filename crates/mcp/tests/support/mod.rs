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
        let has_response_verbosity = extra_args
            .iter()
            .any(|arg| arg.trim() == "--response-verbosity");
        let default_response_verbosity: &[&str] = if has_response_verbosity {
            &[]
        } else {
            &["--response-verbosity", "full"]
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--storage-dir")
            .arg(&storage_dir)
            .args(default_toolset)
            .args(default_response_verbosity)
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

pub(crate) fn claim_job(
    server: &mut Server,
    workspace: &str,
    job_id: &str,
    runner_id: &str,
    lease_ttl_ms: Option<i64>,
    allow_stale: bool,
) -> i64 {
    let mut args = serde_json::Map::new();
    args.insert("workspace".to_string(), json!(workspace));
    args.insert("job".to_string(), json!(job_id));
    args.insert("runner_id".to_string(), json!(runner_id));
    if allow_stale {
        args.insert("allow_stale".to_string(), json!(true));
    }
    if let Some(ttl) = lease_ttl_ms {
        args.insert("lease_ttl_ms".to_string(), json!(ttl));
    }

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": Value::Object(args) } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.claim must succeed: {out}"
    );
    out.get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("job.revision claim token")
}
