#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{SqliteStore, TaskCreateRequest};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_events_sse_is_live_and_does_not_block_snapshot() {
    let base = temp_dir("viewer_events");
    let registry_dir = base.join("registry");
    std::fs::create_dir_all(&registry_dir).expect("create registry dir");
    let repo = create_fake_repo(&base, "repo_a");

    // Seed the store with a workspace so the viewer can resolve defaults.
    let plan_id = {
        let mut store = SqliteStore::open(&repo.storage_dir).expect("open store");
        let workspace = WorkspaceId::try_new("ws-a".to_string()).expect("workspace id");
        store.workspace_init(&workspace).expect("init workspace");
        let (created_plan_id, _, _) = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Plan,
                    title: "Goal Alpha".to_string(),
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .expect("create plan");
        let plan_id = created_plan_id;
        let _ = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Task,
                    title: "Task One".to_string(),
                    parent_plan_id: Some(plan_id.clone()),
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "task_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .expect("create task");
        plan_id
    };

    let Some(port) = pick_free_port() else {
        // Some sandboxed environments disallow TCP bind() even on loopback.
        return;
    };

    let mut proc = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&repo.storage_dir)
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

    wait_for_viewer(port);

    // Open SSE stream.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect sse");
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    write!(
        stream,
        "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
    )
    .expect("write sse request");
    stream.flush().expect("flush sse request");

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .expect("read status line");
    assert!(
        status_line.contains("200"),
        "expected 200 OK, got: {status_line:?}"
    );

    // Read headers.
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("read header");
        if read == 0 {
            panic!("unexpected EOF while reading headers");
        }
        if line.trim().is_empty() {
            break;
        }
    }

    // Wait for the ready event (sent immediately).
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_ready = false;
    while Instant::now() < deadline {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed == "event: ready" {
                    saw_ready = true;
                    break;
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(err) => panic!("read ready event: {err}"),
        }
    }
    assert!(saw_ready, "expected event: ready from /api/events");

    // Ensure the SSE stream does not block other requests.
    let snapshot = http_get_json(port, "/api/snapshot");
    assert_eq!(
        snapshot.get("workspace").and_then(|v| v.as_str()),
        Some("ws-a")
    );

    // Write one more event and expect it to appear on the SSE stream.
    {
        let mut store = SqliteStore::open(&repo.storage_dir).expect("open store for write");
        let workspace = WorkspaceId::try_new("ws-a".to_string()).expect("workspace id");
        let _ = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Task,
                    title: "Task Two".to_string(),
                    parent_plan_id: Some(plan_id.clone()),
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "task_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .expect("create second task");
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_bm_event = false;
    let mut last_id: Option<String> = None;
    while Instant::now() < deadline {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if let Some(rest) = trimmed.strip_prefix("id: ") {
                    last_id = Some(rest.trim().to_string());
                }
                if trimmed == "event: bm_event" {
                    saw_bm_event = true;
                    break;
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(err) => panic!("read bm_event: {err}"),
        }
    }
    assert!(saw_bm_event, "expected event: bm_event from /api/events");
    assert!(
        last_id.unwrap_or_default().starts_with("evt_"),
        "expected an evt_* id before bm_event"
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

fn pick_free_port() -> Option<u16> {
    match std::net::TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => {
            let port = listener.local_addr().expect("local addr").port();
            drop(listener);
            Some(port)
        }
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => None,
        Err(err) => panic!("bind ephemeral port: {err}"),
    }
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
