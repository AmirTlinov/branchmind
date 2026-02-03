#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static SOCKET_SEQ: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn daemon_socket_smoke() {
        let storage_dir = temp_dir("daemon_socket_smoke");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = short_socket_path();

        // Some sandboxed environments disallow unix domain sockets (EPERM). In that case, skip.
        let _ = std::fs::remove_file(&socket_path);
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

        let child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
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

        let stream = wait_for_socket(&socket_path);
        let mut reader = BufReader::new(stream);

        send_frame(
            reader.get_mut(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
            }),
        );
        let _ = recv_frame(&mut reader);

        send_frame(
            reader.get_mut(),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
        );

        send_frame(
            reader.get_mut(),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "ping",
                "params": {}
            }),
        );
        let resp = recv_frame(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));

        cleanup(child, storage_dir, socket_path);
    }

    #[test]
    fn daemon_exits_when_socket_is_unlinked() {
        let storage_dir = temp_dir("daemon_socket_unlink_exits");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = short_socket_path();

        // Some sandboxed environments disallow unix domain sockets (EPERM). In that case, skip.
        let _ = std::fs::remove_file(&socket_path);
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

        let mut child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--daemon")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("daily")
            .arg("--no-viewer")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");

        let _stream = wait_for_socket(&socket_path);
        std::fs::remove_file(&socket_path).expect("unlink socket path");

        // The daemon should notice the missing socket path and exit quickly.
        for _ in 0..80 {
            if let Some(status) = child.try_wait().expect("try_wait") {
                assert!(status.success(), "daemon exited with error: {status}");
                let _ = std::fs::remove_file(&socket_path);
                let _ = std::fs::remove_dir_all(storage_dir);
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket_path);
        panic!("daemon did not exit after socket unlink");
    }

    fn send_frame(stream: &mut UnixStream, value: serde_json::Value) {
        let body = serde_json::to_vec(&value).expect("serialize request");
        write!(stream, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        stream.write_all(&body).expect("write body");
        stream.flush().expect("flush request");
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
        // CI can be noisy; give the daemon a bit more time to bind the socket.
        for _ in 0..200 {
            if let Ok(stream) = UnixStream::connect(path) {
                return stream;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("socket did not become ready");
    }

    fn cleanup(mut child: Child, storage_dir: PathBuf, socket_path: PathBuf) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket_path);
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

    fn short_socket_path() -> PathBuf {
        let pid = std::process::id();
        let seq = SOCKET_SEQ.fetch_add(1, Ordering::Relaxed);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let base = PathBuf::from("/tmp");
        let filename = format!("bm_{pid}_{nonce}_{seq}.sock");
        if base.is_dir() {
            base.join(filename)
        } else {
            std::env::temp_dir().join(filename)
        }
    }
}
