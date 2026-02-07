#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::PathBuf;
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
    use std::time::Duration;

    struct ContentLengthProxy {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    }

    impl ContentLengthProxy {
        fn spawn(storage_dir: &PathBuf, socket_path: &PathBuf) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
                .arg("--shared")
                .arg("--storage-dir")
                .arg(storage_dir)
                .arg("--toolset")
                .arg("full")
                .arg("--socket")
                .arg(socket_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn bm_mcp shared proxy");

            let stdin = child.stdin.take().expect("stdin");
            let stdout = BufReader::new(child.stdout.take().expect("stdout"));

            Self {
                child,
                stdin,
                stdout,
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
                assert!(read > 0, "EOF while reading headers");
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                if let Some((key, value)) = trimmed.split_once(':')
                    && key.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse::<usize>().ok();
                }
            }
            let len = content_length.expect("Content-Length header");
            let mut body = vec![0u8; len];
            self.stdout.read_exact(&mut body).expect("read body");
            serde_json::from_slice(&body).expect("parse response json")
        }

        fn request(&mut self, req: serde_json::Value) -> serde_json::Value {
            self.send(req);
            self.recv()
        }

        fn initialize(&mut self) {
            let init = self.request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
            }));
            assert!(
                init.get("result").is_some(),
                "initialize must return result"
            );

            // Notification => no response.
            self.send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }));
        }

        fn tools_call(
            &mut self,
            id: i64,
            name: &str,
            args: serde_json::Value,
        ) -> serde_json::Value {
            let resp = self.request(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": { "name": name, "arguments": args }
            }));
            assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(id));
            resp
        }

        fn wait_ok(&mut self) {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                panic!("proxy exited early: {status}");
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
        base.join(format!("bm_mcp_shared_cl_{test_name}_{pid}_{nonce}"))
    }

    #[test]
    fn shared_proxy_content_length_two_clients_soak() {
        let storage_dir = temp_dir("shared_proxy_content_length_soak");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("bm.sock");

        let mut a = ContentLengthProxy::spawn(&storage_dir, &socket_path);
        let mut b = ContentLengthProxy::spawn(&storage_dir, &socket_path);
        a.initialize();
        b.initialize();

        // Small concurrent-ish soak: alternate calls and ensure neither proxy exits.
        for i in 0..40 {
            a.wait_ok();
            b.wait_ok();

            let _ = a.tools_call(
                1000 + i,
                "think_card",
                json!({ "workspace": "ws_soak", "card": format!("hello {i}") }),
            );
            let _ = b.tools_call(
                2000 + i,
                "status",
                json!({ "workspace": "ws_soak", "max_chars": 2000 }),
            );
        }

        // Give the daemon a moment to settle and ensure connections remain stable.
        std::thread::sleep(Duration::from_millis(200));
        a.wait_ok();
        b.wait_ok();
        let _ = a.tools_call(
            9001,
            "status",
            json!({ "workspace": "ws_soak", "max_chars": 2000 }),
        );
        let _ = b.tools_call(
            9002,
            "tasks_snapshot",
            json!({ "workspace": "ws_soak", "delta": true, "refs": true, "max_chars": 2000 }),
        );

        let _ = a.child.kill();
        let _ = a.child.wait();
        let _ = b.child.kill();
        let _ = b.child.wait();

        let _ = std::fs::remove_dir_all(&storage_dir);
    }
}
