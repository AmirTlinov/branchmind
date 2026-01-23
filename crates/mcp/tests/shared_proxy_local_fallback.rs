#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};

    #[test]
    fn shared_proxy_can_serve_tools_call_when_daemon_spawn_fails() {
        let storage_dir = temp_dir("shared_proxy_local_fallback");
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

        let proxy_exe = storage_dir.join("bm_mcp_proxy");
        copy_executable(env!("CARGO_BIN_EXE_bm_mcp"), &proxy_exe);

        let mut proxy = Command::new(&proxy_exe)
            .arg("--shared")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--no-viewer")
            // Ensure PATH fallback cannot find `bm_mcp` if daemon spawning breaks.
            .env("PATH", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn proxy");

        // Simulate a common “zombie proxy” failure mode: the proxy process is alive but its
        // on-disk binary path is gone (e.g. rebuild/cleanup). In this case spawning a daemon
        // should fail, but the proxy must still answer tool calls via an in-process fallback.
        std::fs::remove_file(&proxy_exe).expect("unlink proxy binary");

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
        .expect("write initialized notification");
        stdin.flush().expect("flush initialized notification");

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
            "proxy should answer via in-process fallback when daemon is unavailable"
        );

        cleanup(proxy, storage_dir);
    }

    fn copy_executable(from: &str, to: &PathBuf) {
        std::fs::copy(from, to).expect("copy binary");
        let perm = std::fs::metadata(from)
            .expect("read source perms")
            .permissions();
        std::fs::set_permissions(to, perm).expect("set dest perms");
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
