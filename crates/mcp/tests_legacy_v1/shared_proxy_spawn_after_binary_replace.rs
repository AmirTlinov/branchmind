#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    #[test]
    fn shared_proxy_can_spawn_daemon_after_binary_replace() {
        let storage_dir = temp_dir("shared_proxy_spawn_after_binary_replace");
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

        // Run the proxy from a copied binary so we can atomically replace the on-disk path
        // while the process remains alive (simulating a local rebuild that swaps the executable).
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
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn proxy");

        // Replace the proxy binary file at the same path. On Unix, this is a realistic proxy-daemon
        // failure mode: `current_exe()` may become `(... (deleted))` and cannot be spawned.
        let next_exe = storage_dir.join("bm_mcp_proxy.next");
        copy_executable(env!("CARGO_BIN_EXE_bm_mcp"), &next_exe);
        std::fs::rename(&next_exe, &proxy_exe).expect("replace proxy binary");

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

        // This request is forwarded to the daemon; if daemon spawning is broken we would get a -32000
        // transport error. With the fallback fix, the proxy should spawn a fresh daemon successfully.
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
            Some(false)
        );

        shutdown_daemon(&socket_path);
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

    fn shutdown_daemon(socket_path: &PathBuf) {
        // Best-effort: if the daemon never started, don't fail test cleanup.
        let Ok(stream) = wait_for_socket(socket_path) else {
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

    fn wait_for_socket(path: &PathBuf) -> Result<UnixStream, ()> {
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
        base.join(format!(
            "branchmind_{}_{}_{}",
            test_name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ))
    }
}
