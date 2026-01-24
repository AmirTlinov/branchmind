#![forbid(unsafe_code)]

use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct ContentLengthClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    storage_dir: PathBuf,
}

impl ContentLengthClient {
    fn start(test_name: &str) -> Self {
        let storage_dir = temp_dir(test_name);
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");

        let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--no-viewer")
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

    fn send(&mut self, req: serde_json::Value) {
        let body = serde_json::to_vec(&req).expect("serialize request");
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        self.stdin.write_all(&body).expect("write body");
        self.stdin.flush().expect("flush request");
    }

    fn recv(&mut self) -> serde_json::Value {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read header line");
            assert!(read > 0, "unexpected EOF reading response headers");
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':')
                && key.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = Some(value.trim().parse::<usize>().expect("content-length"));
            }
        }

        let len = content_length.expect("missing Content-Length in response");
        let mut buf = vec![0u8; len];
        self.stdout
            .read_exact(&mut buf)
            .expect("read response body");
        serde_json::from_slice(&buf).expect("parse response json")
    }

    fn request(&mut self, req: serde_json::Value) -> serde_json::Value {
        self.send(req);
        self.recv()
    }
}

impl Drop for ContentLengthClient {
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
    base.join(format!("bm_mcp_cl_{test_name}_{pid}_{nonce}"))
}

#[test]
fn mcp_supports_content_length_framing() {
    let mut client = ContentLengthClient::start("content_length_smoke");

    let init = client.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert!(
        init.get("result").is_some(),
        "initialize must return result"
    );

    // `initialized` is a notification (no id) => no response expected.
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    }));

    // Client compatibility: ignore unknown notifications (no response expected).
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {}
    }));

    let tools_list = client.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");
    assert!(
        tools
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("status")),
        "tools/list must include status"
    );
}
