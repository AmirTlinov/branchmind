#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    #[test]
    fn shared_proxy_restarts_daemon_on_project_guard_mismatch() {
        let storage_dir = temp_dir("shared_proxy_guard_switch");
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

        let daemon_guard_a = "repo:aaaaaaaaaaaaaaaa";
        let daemon_guard_b = "repo:bbbbbbbbbbbbbbbb";

        let daemon = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--daemon")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--no-viewer")
            .arg("--workspace")
            .arg("demo")
            .arg("--project-guard")
            .arg(daemon_guard_a)
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
            .arg("--workspace")
            .arg("demo")
            .arg("--project-guard")
            .arg(daemon_guard_b)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn proxy");

        let stdin = proxy.stdin.as_mut().expect("proxy stdin");
        let stdout = proxy.stdout.as_mut().expect("proxy stdout");
        let mut reader = BufReader::new(stdout);

        // Standard MCP handshake so the proxy starts forwarding requests.
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
        .expect("write request");
        stdin.flush().expect("flush request");
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
        .expect("write notification");
        stdin.flush().expect("flush notification");

        // The proxy should have restarted the daemon and the daemon_info should reflect guard_b.
        // CI can be noisy; poll for a short window to avoid flakes.
        let mut guard = String::new();
        for attempt in 0..60 {
            writeln!(
                stdin,
                "{}",
                json!({
                    "jsonrpc": "2.0",
                    "id": 2 + attempt,
                    "method": "branchmind/daemon_info",
                    "params": {}
                })
            )
            .expect("write daemon_info");
            stdin.flush().expect("flush daemon_info");
            let resp = read_line_json(&mut reader);
            guard = resp
                .get("result")
                .and_then(|v| v.get("project_guard"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if guard == daemon_guard_b {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(guard, daemon_guard_b);

        // Clean up: ask the daemon to exit cleanly.
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 99,
                "method": "branchmind/daemon_shutdown",
                "params": {}
            })
        )
        .expect("write daemon_shutdown");
        stdin.flush().expect("flush daemon_shutdown");
        let _ = read_line_json(&mut reader);

        cleanup(proxy, daemon, storage_dir);
    }

    fn read_line_json<R: std::io::Read>(reader: &mut BufReader<R>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response json")
    }

    fn wait_for_socket(path: &PathBuf) {
        for _ in 0..40 {
            if UnixStream::connect(path).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("socket did not become ready");
    }

    fn cleanup(mut proxy: Child, mut daemon: Child, storage_dir: PathBuf) {
        let _ = proxy.kill();
        let _ = proxy.wait();
        let _ = daemon.kill();
        let _ = daemon.wait();
        let _ = std::fs::remove_dir_all(storage_dir);
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
