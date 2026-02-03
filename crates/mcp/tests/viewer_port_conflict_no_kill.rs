#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
    use std::time::{Duration, Instant};

    struct Proxy {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    }

    impl Proxy {
        fn spawn(storage_dir: &PathBuf, socket_path: &PathBuf, viewer_port: u16) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
                .arg("--shared")
                .arg("--storage-dir")
                .arg(storage_dir)
                .arg("--socket")
                .arg(socket_path)
                .arg("--toolset")
                .arg("full")
                .arg("--viewer")
                .arg("--viewer-port")
                .arg(viewer_port.to_string())
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
                "method": "notifications/initialized",
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
            let is_error = resp
                .get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            assert!(!is_error, "status failed: {resp:?}");
        }
    }

    #[test]
    fn viewer_port_conflict_does_not_kill_other_session() {
        let Some(viewer_port) = pick_free_port() else {
            // Some sandboxed environments disallow TCP bind() even on loopback.
            // This test is about viewer session isolation, not OS networking policy.
            return;
        };

        let storage_a = temp_dir("viewer_port_conflict_a");
        let storage_b = temp_dir("viewer_port_conflict_b");
        std::fs::create_dir_all(&storage_a).expect("create storage dir a");
        std::fs::create_dir_all(&storage_b).expect("create storage dir b");
        let socket_a = storage_a.join("branchmind_a.sock");
        let socket_b = storage_b.join("branchmind_b.sock");

        if !preflight_socket(&socket_a, &storage_a) || !preflight_socket(&socket_b, &storage_b) {
            return;
        }

        let mut proxy_a = Proxy::spawn(&storage_a, &socket_a, viewer_port);
        proxy_a.initialize();
        proxy_a.ping(2);
        proxy_a.status(3, "ws");

        // Ensure the viewer port is actually occupied by the first session.
        wait_for_viewer(viewer_port);

        // Start a second session on the same viewer port. This must never terminate proxy_a.
        let mut proxy_b = Proxy::spawn(&storage_b, &socket_b, viewer_port);
        proxy_b.initialize();
        proxy_b.ping(4);
        proxy_b.status(5, "ws");

        std::thread::sleep(Duration::from_millis(150));
        proxy_a.ping(6);
        proxy_a.status(7, "ws");

        let _ = proxy_b.child.kill();
        let _ = proxy_b.child.wait();
        let _ = proxy_a.child.kill();
        let _ = proxy_a.child.wait();

        // Cleanup daemons (spawned by the proxies) so tests don't leak background processes.
        shutdown_daemon(&socket_a);
        shutdown_daemon(&socket_b);
        let _ = std::fs::remove_dir_all(&storage_a);
        let _ = std::fs::remove_dir_all(&storage_b);
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

    fn pick_free_port() -> Option<u16> {
        match std::net::TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => {
                let port = listener.local_addr().expect("local addr").port();
                drop(listener);
                Some(port)
            }
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => None,
            Err(err) => panic!("bind free port: {err}"),
        }
    }

    fn wait_for_viewer(port: u16) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                let _ = stream.write_all(b"GET /api/about HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
                return;
            }
            if Instant::now() >= deadline {
                panic!("viewer did not become reachable on 127.0.0.1:{port}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn shutdown_daemon(path: &PathBuf) {
        let Ok(stream) = wait_for_socket_ready(path) else {
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
        let _ = std::fs::remove_file(path);
    }

    fn wait_for_socket_ready(path: &PathBuf) -> std::io::Result<UnixStream> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match UnixStream::connect(path) {
                Ok(stream) => return Ok(stream),
                Err(err) if Instant::now() < deadline => {
                    if err.kind() == std::io::ErrorKind::PermissionDenied {
                        return Err(err);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err),
            }
        }
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

    fn temp_dir(test_name: &str) -> PathBuf {
        // Prefer a short runtime dir to avoid Unix domain socket SUN_LEN limits on some systems.
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
        base.join(format!("bm_mcp_{test_name}_{pid}_{nonce}"))
    }
}
