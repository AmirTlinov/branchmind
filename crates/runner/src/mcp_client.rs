#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(crate) struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    pub(crate) fn spawn(
        mcp_bin: &str,
        storage_dir: &Path,
        workspace: &str,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(storage_dir)
            .map_err(|e| format!("failed to create storage dir: {e}"))?;

        let mut child = Command::new(mcp_bin)
            .arg("--shared")
            .arg("--storage-dir")
            .arg(storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--workspace")
            .arg(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn bm_mcp ({mcp_bin}): {e}"))?;

        let stdin = child.stdin.take().ok_or("bm_mcp stdin unavailable")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("bm_mcp stdout unavailable")?);

        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        })
    }

    fn send(&mut self, req: Value) -> Result<(), String> {
        writeln!(self.stdin, "{req}").map_err(|e| format!("write request failed: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("flush failed: {e}"))?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| format!("read response failed: {e}"))?;
        if line.trim().is_empty() {
            return Err("empty response line from bm_mcp".to_string());
        }
        serde_json::from_str(&line).map_err(|e| format!("parse response json failed: {e}"))
    }

    fn request(&mut self, req: Value) -> Result<Value, String> {
        self.send(req)?;
        self.recv()
    }

    pub(crate) fn initialize(&mut self) -> Result<(), String> {
        let init_id = self.next_id;
        self.next_id += 1;
        let _ = self.request(json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": super::MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "bm_runner", "version": env!("CARGO_PKG_VERSION") }
            }
        }))?;
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))?;
        Ok(())
    }

    pub(crate) fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))?;

        if resp.get("error").is_some() {
            let msg = resp
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("mcp error");
            return Err(format!("{name} failed: {msg}"));
        }

        let text = resp
            .get("result")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.get(0))
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{name} missing result.content[0].text"))?;

        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            // Most BranchMind tools return an AI-envelope JSON object:
            // { success, intent, result, warnings, ... }.
            // The runner operates on the inner `result` payload.
            if parsed
                .get("success")
                .and_then(|v| v.as_bool())
                .is_some_and(|ok| !ok)
            {
                let msg = parsed
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool error");
                return Err(format!("{name} failed: {msg}"));
            }
            if let Some(inner) = parsed.get("result") {
                return Ok(inner.clone());
            }
            return Ok(parsed);
        }

        Ok(Value::String(text.to_string()))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
