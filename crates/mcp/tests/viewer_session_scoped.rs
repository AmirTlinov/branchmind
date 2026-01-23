#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn viewer_is_session_scoped_in_shared_mode() {
        let storage_dir = temp_dir("viewer_session_scoped");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("branchmind_test.sock");
        let viewer_port = pick_free_port();

        // Some sandboxed environments disallow unix domain sockets (EPERM). In that case, skip.
        match UnixListener::bind(&socket_path) {
            Ok(listener) => {
                drop(listener);
                let _ = std::fs::remove_file(&socket_path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                let _ = std::fs::remove_dir_all(storage_dir);
                return;
            }
            Err(err) => panic!("unix socket bind preflight failed: {err}"),
        }

        let mut proxy = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--shared")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
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

        let stdin = proxy.stdin.as_mut().expect("proxy stdin");
        let stdout = proxy.stdout.as_mut().expect("proxy stdout");
        let mut reader = BufReader::new(stdout);

        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
            })
        )
        .expect("write initialize");
        stdin.flush().expect("flush initialize");
        let _ = read_line_json(&mut reader);

        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            })
        )
        .expect("write initialized");
        stdin.flush().expect("flush initialized");

        // Force shared mode to spawn/connect a daemon.
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "status", "arguments": { "workspace": "ws", "fmt": "json" } }
            })
        )
        .expect("write status");
        stdin.flush().expect("flush status");
        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(false)
        );

        // Viewer should be reachable while the proxy is alive.
        wait_for_viewer(viewer_port);

        // Kill the proxy but keep the daemon alive. The viewer must stop with the session.
        let _ = proxy.kill();
        let _ = proxy.wait();
        wait_for_viewer_closed(viewer_port);

        // Cleanup daemon (spawned by the proxy) so tests don't leak background processes.
        shutdown_daemon(&socket_path);
        let _ = std::fs::remove_dir_all(storage_dir);
    }

    fn read_line_json<R: Read>(reader: &mut BufReader<R>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn pick_free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        port
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

    fn wait_for_viewer_closed(port: u16) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_err() {
                return;
            }
            if Instant::now() >= deadline {
                panic!("viewer did not stop after proxy exit on 127.0.0.1:{port}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn shutdown_daemon(path: &PathBuf) {
        let stream = wait_for_socket(path);
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

    fn wait_for_socket(path: &PathBuf) -> UnixStream {
        for _ in 0..200 {
            if let Ok(stream) = UnixStream::connect(path) {
                return stream;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("socket did not become ready");
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
}
