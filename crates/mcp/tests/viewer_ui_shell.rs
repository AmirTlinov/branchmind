#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_root_is_stub_page() {
    let Some(port) = pick_free_port() else {
        // Some sandboxed environments disallow TCP bind() even on loopback.
        return;
    };

    let storage_dir = temp_dir("viewer_ui_shell");
    std::fs::create_dir_all(&storage_dir).expect("create storage dir");

    let mut proc = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&storage_dir)
        .arg("--toolset")
        .arg("full")
        .arg("--workspace")
        .arg("ws-ui")
        .arg("--viewer")
        .arg("--viewer-port")
        .arg(port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bm_mcp");

    wait_for_viewer(port);
    let index = http_get_text(port, "/");

    // Viewer UI is now a separate desktop app; the MCP server exposes a minimal
    // root HTML page + read-only API under /api/*.
    assert!(
        index.contains("BranchMind Viewer API"),
        "expected viewer root stub page"
    );
    assert!(
        index.contains("make run-viewer-tauri"),
        "expected viewer root stub to point to the desktop app entrypoint"
    );

    let _ = proc.kill();
    let _ = proc.wait();
    let _ = std::fs::remove_dir_all(storage_dir);
}

fn wait_for_viewer(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("viewer did not become reachable on 127.0.0.1:{port}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn http_get_text(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let _ = stream.set_read_timeout(Some(Duration::from_millis(700)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(700)));
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .expect("write request");
    stream.flush().expect("flush request");

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .expect("read status line");
    assert!(
        status_line.contains("200"),
        "expected 200 from {path}, got: {status_line:?}"
    );

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("read header");
        if read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':')
            && key.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let mut body = Vec::new();
    if let Some(len) = content_length {
        body.resize(len, 0);
        reader.read_exact(&mut body).expect("read body");
    } else {
        reader.read_to_end(&mut body).expect("read body");
    }

    String::from_utf8_lossy(&body).to_string()
}

fn pick_free_port() -> Option<u16> {
    match std::net::TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener.local_addr().ok().map(|addr| addr.port()),
        Err(_) => None,
    }
}

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    base.join(format!("bm_viewer_{test_name}_{pid}_{nonce}"))
}
