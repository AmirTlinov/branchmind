#![forbid(unsafe_code)]

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_projects_endpoint_supports_multi_project_selection() {
    let base = temp_dir("viewer_projects");
    let registry_dir = base.join("registry");
    std::fs::create_dir_all(&registry_dir).expect("create registry dir");

    let repo_a = create_fake_repo(&base, "repo_a");
    let repo_b = create_fake_repo(&base, "repo_b");
    let port = pick_free_port();

    let mut proc_a = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&repo_a.storage_dir)
        .arg("--toolset")
        .arg("full")
        .arg("--workspace")
        .arg("ws-a")
        .arg("--viewer")
        .arg("--viewer-port")
        .arg(port.to_string())
        .env("BRANCHMIND_VIEWER_REGISTRY_DIR", &registry_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn viewer process");

    let mut proc_b = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&repo_b.storage_dir)
        .arg("--toolset")
        .arg("full")
        .arg("--workspace")
        .arg("ws-b")
        .arg("--no-viewer")
        .env("BRANCHMIND_VIEWER_REGISTRY_DIR", &registry_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn background project process");

    wait_for_viewer(port);
    let projects = wait_for_projects(port, 2);

    let current_guard = projects
        .get("current_project_guard")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        current_guard.starts_with("repo:"),
        "expected current_project_guard to be set"
    );

    let list = projects
        .get("projects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        list.len() >= 2,
        "expected at least 2 projects in /api/projects (got {})",
        list.len()
    );

    let other = list
        .iter()
        .filter_map(|v| v.as_object())
        .find(|obj| {
            obj.get("project_guard").and_then(|v| v.as_str()) != Some(current_guard.as_str())
        })
        .expect("expected another project besides current");
    let other_guard = other
        .get("project_guard")
        .and_then(|v| v.as_str())
        .expect("project_guard")
        .to_string();

    // Percent-decoding is required because UI uses encodeURIComponent (":" -> "%3A").
    let encoded_guard = other_guard.replace(':', "%3A");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = http_get_json(port, &format!("/api/snapshot?project={encoded_guard}"));
        if snapshot.get("workspace").and_then(|v| v.as_str()) == Some("ws-b") {
            break;
        }
        let err_code = snapshot
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if err_code == "PROJECT_UNAVAILABLE" && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        panic!("expected snapshot for other project (ws-b), got: {snapshot:?}");
    }

    let _ = proc_a.kill();
    let _ = proc_b.kill();
    let _ = proc_a.wait();
    let _ = proc_b.wait();
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

fn wait_for_projects(port: u16, min: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let payload = http_get_json(port, "/api/projects");
        let count = payload
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        if count >= min {
            return payload;
        }
        if Instant::now() >= deadline {
            panic!("expected >= {min} projects, got {count}");
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

struct FakeRepo {
    #[allow(dead_code)]
    root: PathBuf,
    storage_dir: PathBuf,
}

fn create_fake_repo(base: &Path, name: &str) -> FakeRepo {
    let root = base.join(name);
    std::fs::create_dir_all(root.join(".git")).expect("create fake .git");
    let storage_dir = root.join(".agents").join("mcp").join(".branchmind");
    std::fs::create_dir_all(&storage_dir).expect("create storage dir");
    FakeRepo { root, storage_dir }
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
