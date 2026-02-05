#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::{SqliteStore, ThinkCardCommitRequest, ThinkCardInput};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_snapshot_supports_knowledge_lens() {
    let base = temp_dir("viewer_lens");
    let registry_dir = base.join("registry");
    std::fs::create_dir_all(&registry_dir).expect("create registry dir");
    let repo = create_fake_repo(&base, "repo_a");

    let workspace = WorkspaceId::try_new("ws-a".to_string()).expect("workspace id");
    let (card_id, anchor_id, key_slug) = {
        let mut store = SqliteStore::open(&repo.storage_dir).expect("open store");
        store.workspace_init(&workspace).expect("init workspace");

        let card_id = store.next_card_id(&workspace).expect("next card id");
        store
            .think_card_commit(
                &workspace,
                ThinkCardCommitRequest {
                    branch: "main".to_string(),
                    trace_doc: "kb-trace".to_string(),
                    graph_doc: "kb-graph".to_string(),
                    card: ThinkCardInput {
                        card_id: card_id.clone(),
                        card_type: "knowledge".to_string(),
                        title: Some("Viewer live updates".to_string()),
                        text: Some("SSE events stream + snapshot refresh.".to_string()),
                        status: None,
                        tags: vec![
                            "a:viewer".to_string(),
                            "k:events-sse-live".to_string(),
                            "v:canon".to_string(),
                        ],
                        meta_json: None,
                        content: "".to_string(),
                        payload_json: "{}".to_string(),
                    },
                    supports: Vec::new(),
                    blocks: Vec::new(),
                },
            )
            .expect("commit think card");
        (
            card_id,
            "a:viewer".to_string(),
            "events-sse-live".to_string(),
        )
    };

    let Some(port) = pick_free_port() else {
        // Some sandboxed environments disallow TCP bind() even on loopback.
        // This test is about viewer lens routing, not OS networking policy.
        return;
    };

    let mut proc = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--storage-dir")
        .arg(&repo.storage_dir)
        .arg("--toolset")
        .arg("full")
        .arg("--workspace")
        .arg(workspace.as_str())
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

    let payload = http_get_json(port, "/api/snapshot?lens=knowledge");
    assert_eq!(
        payload.get("lens").and_then(|v| v.as_str()),
        Some("knowledge"),
        "expected lens=knowledge"
    );

    let plans = payload
        .get("plans")
        .and_then(|v| v.as_array())
        .expect("plans array");
    assert!(
        plans
            .iter()
            .any(|plan| plan.get("id").and_then(|v| v.as_str()) == Some(anchor_id.as_str())),
        "expected anchor id in plans"
    );

    let tasks = payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .expect("tasks array");
    let saw_key = tasks.iter().any(|task| {
        task.get("plan_id").and_then(|v| v.as_str()) == Some(anchor_id.as_str())
            && task.get("card_id").and_then(|v| v.as_str()) == Some(card_id.as_str())
            && task.get("key").and_then(|v| v.as_str()) == Some(key_slug.as_str())
    });
    assert!(saw_key, "expected knowledge key task row");

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
    let _ = stream.set_read_timeout(Some(Duration::from_millis(600)));

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    stream.flush().expect("flush");

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    let (_head, body) = response
        .split_once("\r\n\r\n")
        .expect("http response split");
    serde_json::from_str(body).expect("parse json response")
}

fn pick_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

struct FakeRepo {
    #[allow(dead_code)]
    root: PathBuf,
    storage_dir: PathBuf,
}

fn create_fake_repo(base: &Path, name: &str) -> FakeRepo {
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("create repo dir");
    std::fs::write(root.join("README.md"), "# fake repo\n").expect("seed readme");
    let storage_dir = root.join(".branchmind");
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
