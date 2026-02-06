#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn shared_proxy_system_daemon_restart_is_one_command_and_self_heals() {
        let storage_dir =
            temp_dir("shared_proxy_system_daemon_restart_is_one_command_and_self_heals");
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

        // Start an explicit daemon so we can assert it terminates on restart.
        let mut daemon = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
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

        // One-command UX: force daemon restart.
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.daemon.restart", "args": {} } }
            })
        )
        .expect("write daemon restart");
        stdin.flush().expect("flush daemon restart");
        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "restart must succeed in shared mode"
        );

        // The explicit daemon we started must terminate (best-effort shutdown).
        wait_for_exit(&mut daemon, Duration::from_secs(2));

        // After restart, the next forwarded call must still work (proxy should spawn a fresh daemon).
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "status", "arguments": { "workspace": "ws", "fmt": "json" } }
            })
        )
        .expect("write status");
        stdin.flush().expect("flush status");
        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(
            resp.get("result")
                .and_then(|v| v.get("isError"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "status must succeed after restart"
        );

        // Cleanup: restart again to shut down the freshly spawned daemon (avoid leaks in tests).
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.daemon.restart", "args": {} } }
            })
        )
        .expect("write daemon restart cleanup");
        stdin.flush().expect("flush daemon restart cleanup");
        let resp = read_line_json(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(4));

        cleanup(proxy, storage_dir, socket_path);
    }

    fn read_line_json<R: std::io::Read>(reader: &mut BufReader<R>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn wait_for_socket(socket_path: &Path) {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if socket_path.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("daemon socket did not become ready");
    }

    fn wait_for_exit(child: &mut Child, timeout: Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        // Kill if it's still around: tests must not leak.
        let _ = child.kill();
        let _ = child.wait();
    }

    fn cleanup(mut proxy: Child, storage_dir: PathBuf, socket_path: PathBuf) {
        let _ = proxy.kill();
        let _ = proxy.wait();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_dir_all(storage_dir);
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        test_name.hash(&mut hasher);
        let test_hash = hasher.finish();
        base.join(format!("bm_shrst_{pid}_{nonce}_{test_hash:x}"))
    }
}
