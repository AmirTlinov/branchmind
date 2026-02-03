#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
    use std::time::Duration;

    struct Proxy {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    }

    impl Proxy {
        fn spawn(
            storage_dir: &PathBuf,
            socket_path: Option<&PathBuf>,
            workspace: Option<&str>,
        ) -> Self {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_bm_mcp"));
            cmd.arg("--shared")
                .arg("--storage-dir")
                .arg(storage_dir)
                .arg("--toolset")
                .arg("full")
                .arg("--no-viewer");
            if let Some(socket_path) = socket_path {
                cmd.arg("--socket").arg(socket_path);
            }
            if let Some(workspace) = workspace {
                cmd.arg("--workspace").arg(workspace);
            }

            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn proxy");

            let stdin = child.stdin.take().expect("proxy stdin");
            let stdout = BufReader::new(child.stdout.take().expect("proxy stdout"));

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
                "method": "initialized",
                "params": {}
            }));
        }

        fn send(&mut self, value: serde_json::Value) {
            writeln!(self.stdin, "{value}").expect("write request");
            self.stdin.flush().expect("flush request");
        }

        fn recv(&mut self) -> serde_json::Value {
            let mut line = String::new();
            self.stdout.read_line(&mut line).expect("read response");
            assert!(!line.trim().is_empty(), "empty response line");
            serde_json::from_str(&line).expect("parse response json")
        }

        fn request(&mut self, value: serde_json::Value) -> serde_json::Value {
            self.send(value);
            self.recv()
        }

        fn ping(&mut self, id: i64) {
            let resp = self.request(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "ping",
                "params": {}
            }));
            assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(id));
        }

        fn status(&mut self, id: i64, workspace: &str) {
            let resp = self.request(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": { "name": "status", "arguments": { "workspace": workspace, "fmt": "json" } }
            }));
            assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(id));
        }

        fn think_card(&mut self, id: i64, workspace: &str, text: &str) {
            let resp = self.request(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": "think",
                    "arguments": {
                        "workspace": workspace,
                        "op": "call",
                        "cmd": "think.card",
                        "args": { "card": text }
                    }
                }
            }));
            assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(id));
            let is_error = resp
                .get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            assert!(!is_error, "think_card failed: {resp:?}");
        }
    }

    #[test]
    fn shared_proxy_multi_session_smoke() {
        let storage_dir = temp_dir("shared_proxy_multi_session_smoke");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("branchmind_multi.sock");

        if !preflight_socket(&socket_path, &storage_dir) {
            return;
        }

        let mut daemon = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--daemon")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--workspace")
            .arg("ws_multi")
            .arg("--no-viewer")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");

        wait_for_socket(&socket_path);

        let mut proxies = Vec::new();
        for idx in 0..5 {
            let mut proxy = Proxy::spawn(&storage_dir, Some(&socket_path), Some("ws_multi"));
            proxy.initialize();
            proxy.ping(2);
            proxy.status(3, "ws_multi");
            let text = format!("hello from proxy {idx}");
            proxy.think_card(100 + idx as i64, "ws_multi", &text);
            proxies.push(proxy);
        }

        let mut late_proxy = Proxy::spawn(&storage_dir, Some(&socket_path), Some("ws_multi"));
        late_proxy.initialize();
        late_proxy.ping(4);
        late_proxy.think_card(200, "ws_multi", "hello from late proxy");

        for (idx, proxy) in proxies.iter_mut().enumerate() {
            proxy.ping(10 + idx as i64);
            proxy.status(20 + idx as i64, "ws_multi");
            let text = format!("post-join card {idx}");
            proxy.think_card(300 + idx as i64, "ws_multi", &text);
        }

        cleanup_proxies(proxies);
        cleanup_proxies(vec![late_proxy]);
        shutdown_daemon(&socket_path);
        cleanup_daemon(&mut daemon, storage_dir);
    }

    #[test]
    fn shared_proxy_multi_workspace_e2e() {
        let storage_dir = temp_dir("shared_proxy_multi_workspace_e2e");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let preflight_path = storage_dir.join("branchmind_preflight.sock");
        if !preflight_socket(&preflight_path, &storage_dir) {
            return;
        }

        let mut proxy_a = Proxy::spawn(&storage_dir, None, Some("ws_alpha"));
        proxy_a.initialize();
        proxy_a.status(2, "ws_alpha");
        proxy_a.think_card(10, "ws_alpha", "alpha card");

        let mut proxy_b = Proxy::spawn(&storage_dir, None, Some("ws_beta"));
        proxy_b.initialize();
        proxy_b.status(3, "ws_beta");
        proxy_b.think_card(11, "ws_beta", "beta card");

        proxy_a.ping(4);
        proxy_a.status(5, "ws_alpha");
        proxy_a.think_card(12, "ws_alpha", "alpha card 2");

        cleanup_proxies(vec![proxy_a, proxy_b]);
        shutdown_daemons_in_dir(&storage_dir);
        let _ = std::fs::remove_dir_all(&storage_dir);
    }

    fn preflight_socket(socket_path: &PathBuf, storage_dir: &PathBuf) -> bool {
        match UnixListener::bind(socket_path) {
            Ok(listener) => {
                drop(listener);
                let _ = std::fs::remove_file(socket_path);
                true
            }
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                let _ = std::fs::remove_dir_all(storage_dir);
                false
            }
            Err(err) => panic!("unix socket bind preflight failed: {err}"),
        }
    }

    fn wait_for_socket(path: &PathBuf) {
        for _ in 0..80 {
            if UnixStream::connect(path).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("socket did not become ready");
    }

    fn shutdown_daemons_in_dir(storage_dir: &PathBuf) {
        let Ok(entries) = std::fs::read_dir(storage_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if (name.starts_with("branchmind_mcp") || name.starts_with("bm."))
                && name.ends_with(".sock")
            {
                shutdown_daemon(&path);
            }
        }
    }

    fn shutdown_daemon(socket_path: &PathBuf) {
        let Ok(stream) = wait_for_socket_ready(socket_path) else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        send_frame(
            &stream,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "branchmind/daemon_shutdown",
                "params": {}
            }),
        );
        let _ = recv_frame(&mut reader);
    }

    fn wait_for_socket_ready(path: &PathBuf) -> Result<UnixStream, ()> {
        for _ in 0..80 {
            if let Ok(stream) = UnixStream::connect(path) {
                return Ok(stream);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Err(())
    }

    fn send_frame(stream: &UnixStream, value: serde_json::Value) {
        let body = serde_json::to_vec(&value).expect("serialize request");
        let mut writer = stream;
        write!(writer, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        writer.write_all(&body).expect("write body");
        writer.flush().expect("flush request");
    }

    fn recv_frame(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).expect("read header line");
            assert!(read > 0, "unexpected EOF reading response headers");
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':')
                && key.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = Some(value.trim().parse().expect("content length"));
            }
        }
        let len = content_length.expect("missing content length");
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).expect("read response body");
        serde_json::from_slice(&body).expect("parse response json")
    }

    fn cleanup_proxies(mut proxies: Vec<Proxy>) {
        for proxy in proxies.iter_mut() {
            let _ = proxy.child.kill();
            let _ = proxy.child.wait();
        }
    }

    fn cleanup_daemon(child: &mut Child, storage_dir: PathBuf) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(storage_dir);
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        // Some macOS/sandboxed environments have very long temp dirs (e.g. `/var/folders/...`)
        // that can exceed Unix domain socket path limits when we place `.sock` files under them.
        // Prefer a short runtime dir when available.
        let base = {
            let tmp = PathBuf::from("/tmp");
            if tmp.is_dir() {
                tmp
            } else {
                std::env::temp_dir()
            }
        };
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let dir = base.join(format!("bm_mcp_{test_name}_{pid}_{nonce}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
