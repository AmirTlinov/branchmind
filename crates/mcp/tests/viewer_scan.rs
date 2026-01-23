#![forbid(unsafe_code)]

use bm_storage::SqliteStore;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_projects_endpoint_includes_scanned_non_git_projects() {
    let base = temp_dir("viewer_scan");
    let scan_root = base.join("scan_root");
    std::fs::create_dir_all(&scan_root).expect("create scan root");

    let catalog_dir = base.join("catalog");
    std::fs::create_dir_all(&catalog_dir).expect("create catalog dir");

    let current = scan_root.join("current_project");
    std::fs::create_dir_all(current.join(".git")).expect("create fake .git");
    let current_storage = current.join(".branchmind_rust");
    let _ = SqliteStore::open(&current_storage).expect("create current store");

    let nogit = scan_root.join("nogit_project");
    std::fs::create_dir_all(&nogit).expect("create non-git project");
    let nogit_storage = nogit.join(".branchmind_rust");
    let _ = SqliteStore::open(&nogit_storage).expect("create non-git store");

    // Add one broken catalog entry to ensure store_present is computed.
    let missing_storage = scan_root.join("missing_project").join(".branchmind_rust");
    let missing_entry = serde_json::json!({
        "project_guard": "repo:deadbeef00000000",
        "label": "missing_project",
        "storage_dir": missing_storage.to_string_lossy(),
        "workspace_default": "missing_project",
        "workspace_recommended": "missing_project",
        "updated_at_ms": 0,
        "pid": 0,
        "mode": "scan"
    });
    std::fs::write(catalog_dir.join("missing.json"), missing_entry.to_string())
        .expect("write missing entry");

    let port = pick_free_port();
    let mut proc = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&current_storage)
        .arg("--toolset")
        .arg("full")
        .arg("--workspace")
        .arg("current_project")
        .arg("--viewer")
        .arg("--viewer-port")
        .arg(port.to_string())
        .env("BRANCHMIND_VIEWER_CATALOG_DIR", &catalog_dir)
        .env(
            "BRANCHMIND_VIEWER_SCAN_ROOTS",
            scan_root.to_string_lossy().to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn viewer process");

    wait_for_viewer(port);

    let projects = wait_for_project_label(port, "nogit_project");
    let list = projects
        .get("projects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let nogit_item = list
        .iter()
        .filter_map(|v| v.as_object())
        .find(|obj| obj.get("label").and_then(|v| v.as_str()) == Some("nogit_project"))
        .expect("expected nogit_project to be discoverable via scan");
    assert_eq!(
        nogit_item.get("store_present").and_then(|v| v.as_bool()),
        Some(true),
        "expected scanned project to be marked store_present"
    );

    let missing_item = list
        .iter()
        .filter_map(|v| v.as_object())
        .find(|obj| obj.get("label").and_then(|v| v.as_str()) == Some("missing_project"))
        .expect("expected missing_project entry to be present");
    assert_eq!(
        missing_item.get("store_present").and_then(|v| v.as_bool()),
        Some(false),
        "expected missing project to be marked store_present=false"
    );

    let _ = proc.kill();
    let _ = proc.wait();
    let _ = std::fs::remove_dir_all(base);
}

fn wait_for_viewer(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
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

fn wait_for_project_label(port: u16, label: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let payload = http_get_json(port, "/api/projects");
        let found = payload
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|projects| {
                projects
                    .iter()
                    .any(|item| item.get("label").and_then(|v| v.as_str()) == Some(label))
            })
            .unwrap_or(false);
        if found {
            return payload;
        }
        if Instant::now() >= deadline {
            panic!("expected {label} to be listed in /api/projects");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn http_get_json(port: u16, path: &str) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
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

    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
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
