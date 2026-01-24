#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;

    #[test]
    fn shared_proxy_does_not_kill_daemon_on_daemon_info_timeout() {
        let storage_dir = temp_dir("shared_proxy_daemon_info_fail_open");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("branchmind_test.sock");

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

        // Fake daemon: accept one connection, intentionally do NOT respond to daemon_info so the
        // proxy probe times out, but still answer the subsequent forwarded request.
        let listener = UnixListener::bind(&socket_path).expect("bind fake daemon");
        let (tx, rx) = mpsc::channel::<(String, String)>();
        let daemon_thread = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept connection");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut writer = stream;

            // 1) daemon_info probe (we read it but intentionally do NOT answer).
            let first = recv_frame(&mut reader);
            let first_method = first
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let _ = tx.send((first_method.clone(), "seen".to_string()));

            // 2) First real forwarded request: reply with a generic error so the proxy can return.
            let second = recv_frame(&mut reader);
            let second_method = second
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let _ = tx.send((first_method, second_method.clone()));

            let id = second.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let resp = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "fake daemon" }
            });
            send_frame(&mut writer, resp);
        });

        let mut proxy = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--shared")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--no-viewer")
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
                "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
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
        .expect("write initialized notification");
        stdin.flush().expect("flush initialized notification");

        // Trigger daemon connect + daemon_info probe + forward.
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
        .expect("write tools/call");
        stdin.flush().expect("flush tools/call");

        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));

        // The probe should be the first request, and a probe failure must NOT cause a shutdown.
        let (probe_method, _) = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recv probe method");
        assert_eq!(probe_method, "branchmind/daemon_info");

        let (_probe_method, second_method) = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recv second method");
        assert_ne!(
            second_method.as_str(),
            "branchmind/daemon_shutdown",
            "proxy must not kill daemon on daemon_info probe timeout"
        );

        let _ = daemon_thread.join();
        cleanup(proxy, storage_dir);
    }

    #[test]
    fn shared_proxy_recovers_quickly_from_unresponsive_daemon() {
        let storage_dir = temp_dir("shared_proxy_timeout_recovery");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("branchmind_timeout.sock");

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

        // Fake daemon: accept one connection and never respond. The proxy must time out fast,
        // recover (spawn a real daemon), and still return a response to the client.
        let listener = UnixListener::bind(&socket_path).expect("bind fake daemon");
        let (tx, rx) = mpsc::channel::<String>();
        let daemon_thread = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept connection");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let _writer = stream;

            let first = recv_frame(&mut reader);
            let _ = tx.send(
                first
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
            let second = recv_frame(&mut reader);
            let _ = tx.send(
                second
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            );

            // Keep the connection open long enough for the proxy timeout to trigger.
            thread::sleep(Duration::from_secs(8));
        });

        let mut proxy = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--shared")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("daily")
            .arg("--no-viewer")
            // Ensure any daemon spawned by this proxy exits quickly after the proxy is gone.
            .env("BRANCHMIND_MCP_DAEMON_IDLE_EXIT_SECS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn proxy");

        let stdin = proxy.stdin.as_mut().expect("proxy stdin");
        let stdout = proxy.stdout.take().expect("proxy stdout");
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
        .expect("write initialized notification");
        stdin.flush().expect("flush initialized notification");

        let started = Instant::now();
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "status", "arguments": { "workspace": "ws", "max_chars": 2000 } }
            })
        )
        .expect("write tools/call");
        stdin.flush().expect("flush tools/call");

        let (resp_tx, resp_rx) = mpsc::channel::<serde_json::Value>();
        let join = thread::spawn(move || {
            let resp = read_line_json(&mut reader);
            let _ = resp_tx.send(resp);
        });
        let resp = resp_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("timeout waiting for proxy response");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(15),
            "expected recovery under 15s, got {elapsed:?}"
        );
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));
        let _ = join.join();

        let first_method = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recv first method");
        assert_eq!(first_method, "branchmind/daemon_info");
        let second_method = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recv second method");
        assert_eq!(second_method, "tools/call");

        let _ = daemon_thread.join();
        cleanup(proxy, storage_dir);
    }

    fn send_frame(stream: &mut UnixStream, value: serde_json::Value) {
        let body = serde_json::to_vec(&value).expect("serialize response");
        write!(stream, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        stream.write_all(&body).expect("write body");
        stream.flush().expect("flush");
    }

    fn recv_frame(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).expect("read header line");
            assert!(read > 0, "unexpected EOF reading request headers");
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
        reader.read_exact(&mut body).expect("read request body");
        serde_json::from_slice(&body).expect("parse request json")
    }

    fn read_line_json<R: std::io::Read>(reader: &mut BufReader<R>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn cleanup(mut proxy: Child, storage_dir: PathBuf) {
        let _ = proxy.kill();
        let _ = proxy.wait();
        let _ = std::fs::remove_dir_all(storage_dir);
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "branchmind_{}_{}_{}",
            test_name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        dir
    }
}
