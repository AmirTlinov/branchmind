#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn shared_proxy_restarts_stale_daemon_when_started_from_newer_binary() {
        let storage_dir =
            temp_dir("shared_proxy_restarts_stale_daemon_when_started_from_newer_binary");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        // Keep unix socket paths short (SUN_LEN is typically ~108 bytes).
        let socket_path = storage_dir.join("bm.sock");

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

        // Prepare two executables with different mtimes (our build_time_ms tiebreaker).
        let daemon_exe = storage_dir.join("bm_mcp.daemon");
        copy_executable(env!("CARGO_BIN_EXE_bm_mcp"), &daemon_exe);
        let daemon_mtime_ms = modified_ms(&daemon_exe);

        let proxy_exe = storage_dir.join("bm_mcp.proxy");
        copy_executable_newer_than(env!("CARGO_BIN_EXE_bm_mcp"), &proxy_exe, daemon_mtime_ms);

        // Start an explicit daemon using the older on-disk executable.
        let mut daemon = Command::new(&daemon_exe)
            .arg("--daemon")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--no-viewer")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");

        wait_for_socket(&socket_path);

        // Start a proxy from the newer executable. It should detect the stale daemon (older
        // build_time_ms) and replace it automatically.
        let mut proxy = Command::new(&proxy_exe)
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

        // Handshake (initialize is handled locally by the proxy).
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

        // The first forwarded call must work even if the proxy decides to restart the daemon.
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
        .expect("write tools/call status");
        stdin.flush().expect("flush tools/call status");
        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "status must succeed even if daemon is restarted"
        );

        // The explicit daemon we started should have been terminated (best-effort shutdown).
        assert_exit(&mut daemon, Duration::from_secs(2));

        shutdown_daemon(&socket_path);
        cleanup(proxy, storage_dir);
    }

    fn modified_ms(path: &PathBuf) -> u64 {
        let meta = std::fs::metadata(path).expect("read metadata");
        let modified = meta.modified().expect("read modified time");
        let dur = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
        dur.as_millis().min(u64::MAX as u128) as u64
    }

    fn copy_executable(from: &str, to: &PathBuf) {
        std::fs::copy(from, to).expect("copy binary");
        let perm = std::fs::metadata(from)
            .expect("read source perms")
            .permissions();
        std::fs::set_permissions(to, perm).expect("set dest perms");
    }

    fn copy_executable_newer_than(from: &str, to: &PathBuf, older_ms: u64) {
        let start = Instant::now();
        loop {
            let _ = std::fs::remove_file(to);
            copy_executable(from, to);
            let mtime = modified_ms(to);
            if mtime > older_ms {
                return;
            }
            if start.elapsed() > Duration::from_secs(3) {
                panic!("executable mtime did not advance: old={older_ms} new={mtime}");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn read_line_json<R: Read>(reader: &mut BufReader<R>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn assert_exit(child: &mut Child, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        // Kill if it's still around: tests must not leak. Then fail closed.
        let _ = child.kill();
        let _ = child.wait();
        panic!("expected daemon to exit after stale takeover");
    }

    fn wait_for_socket(path: &PathBuf) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if UnixStream::connect(path).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("daemon socket did not become ready");
    }

    fn shutdown_daemon(socket_path: &PathBuf) {
        // Best-effort: if the daemon never started, don't fail test cleanup.
        let Ok(stream) = wait_for_socket_stream(socket_path) else {
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

    fn wait_for_socket_stream(path: &PathBuf) -> Result<UnixStream, ()> {
        for _ in 0..80 {
            if let Ok(stream) = UnixStream::connect(path) {
                return Ok(stream);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Err(())
    }

    fn cleanup(mut proxy: Child, storage_dir: PathBuf) {
        let _ = proxy.kill();
        let _ = proxy.wait();
        let _ = std::fs::remove_dir_all(storage_dir);
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
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        test_name.hash(&mut hasher);
        let test_hash = hasher.finish();
        base.join(format!("bm_shst_{pid}_{nonce}_{test_hash:x}"))
    }
}
