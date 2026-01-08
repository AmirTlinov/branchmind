#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    #[test]
    fn daemon_socket_smoke() {
        let storage_dir = temp_dir("daemon_socket_smoke");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        let socket_path = storage_dir.join("branchmind_test.sock");

        let child = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
            .arg("--daemon")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--storage-dir")
            .arg(&storage_dir)
            .arg("--toolset")
            .arg("full")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");

        let stream = wait_for_socket(&socket_path);
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        send_frame(
            &stream,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
            }),
        );
        let _ = recv_frame(&mut reader);

        send_frame(
            &stream,
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
        );

        send_frame(
            &stream,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "ping",
                "params": {}
            }),
        );
        let resp = recv_frame(&mut reader);
        assert_eq!(resp.get("id").and_then(|v| v.as_i64()), Some(2));

        cleanup(child, storage_dir);
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
            if let Some((key, value)) = trimmed.split_once(':') {
                if key.trim().eq_ignore_ascii_case("content-length") {
                    content_length = Some(value.trim().parse().expect("content length"));
                }
            }
        }
        let len = content_length.expect("missing content length");
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).expect("read response body");
        serde_json::from_slice(&body).expect("parse response json")
    }

    fn wait_for_socket(path: &PathBuf) -> UnixStream {
        for _ in 0..40 {
            if let Ok(stream) = UnixStream::connect(path) {
                return stream;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("socket did not become ready");
    }

    fn cleanup(mut child: Child, storage_dir: PathBuf) {
        let _ = child.kill();
        let _ = child.wait();
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
