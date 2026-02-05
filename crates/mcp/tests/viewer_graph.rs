#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{SqliteStore, TaskCreateRequest};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn viewer_graph_endpoints_page_and_resolve_local_cluster() {
    let base = temp_dir("viewer_graph");
    let registry_dir = base.join("registry");
    std::fs::create_dir_all(&registry_dir).expect("create registry dir");
    let repo = create_fake_repo(&base, "repo_a");

    let workspace = WorkspaceId::try_new("ws-a".to_string()).expect("workspace id");
    let (plan_id, task_id) = {
        let mut store = SqliteStore::open(&repo.storage_dir).expect("open store");
        store.workspace_init(&workspace).expect("init workspace");

        let (plan_id, _, _) = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Plan,
                    title: "Goal Graph".to_string(),
                    parent_plan_id: None,
                    description: Some("Graph subgraph paging".to_string()),
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .expect("create plan");

        let mut first_task_id = None::<String>;
        for idx in 0..7usize {
            let (task_id, _, _) = store
                .create(
                    &workspace,
                    TaskCreateRequest {
                        kind: TaskKind::Task,
                        title: format!("Alpha task {idx}"),
                        parent_plan_id: Some(plan_id.clone()),
                        description: Some("tile alpha beta".to_string()),
                        contract: None,
                        contract_json: None,
                        event_type: "task_created".to_string(),
                        event_payload_json: "{}".to_string(),
                    },
                )
                .expect("create task");
            if first_task_id.is_none() {
                first_task_id = Some(task_id);
            }
        }

        (plan_id, first_task_id.expect("task id"))
    };

    let Some(port) = pick_free_port() else {
        // Some sandboxed environments disallow TCP bind() even on loopback.
        // This test is about viewer endpoint wiring, not OS networking policy.
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

    // Plan paging.
    let page1 = http_get_json(
        port,
        &format!("/api/graph/plan/{plan_id}?lens=work&limit=3"),
    );
    assert_eq!(
        page1.get("lens").and_then(|v| v.as_str()),
        Some("work"),
        "expected lens=work"
    );
    assert_eq!(
        page1
            .get("plan")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(plan_id.as_str()),
        "expected plan id"
    );
    let tasks1 = page1
        .get("tasks")
        .and_then(|v| v.as_array())
        .expect("tasks array");
    assert_eq!(tasks1.len(), 3, "expected limit=3 tasks");
    let cursor1 = page1
        .get("pagination")
        .and_then(|v| v.get("next_cursor"))
        .and_then(|v| v.as_str())
        .expect("next_cursor");

    let page2 = http_get_json(
        port,
        &format!("/api/graph/plan/{plan_id}?lens=work&limit=3&cursor={cursor1}"),
    );
    let tasks2 = page2
        .get("tasks")
        .and_then(|v| v.as_array())
        .expect("tasks array");
    assert!(
        !tasks2.is_empty(),
        "expected at least one task on second page"
    );
    let ids1 = tasks1
        .iter()
        .filter_map(|task| task.get("id").and_then(|v| v.as_str()))
        .collect::<HashSet<_>>();
    for task in tasks2.iter() {
        let id = task.get("id").and_then(|v| v.as_str()).expect("task id");
        assert!(!ids1.contains(id), "expected paging without duplicates");
        assert!(
            id > cursor1,
            "expected task id to be > cursor (lexicographic, stable)"
        );
    }

    // Local graph for a task and its cluster.
    let local = http_get_json(
        port,
        &format!("/api/graph/local/{task_id}?lens=work&hops=2&limit=40"),
    );
    assert_eq!(
        local
            .get("root")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str()),
        "expected local root id"
    );
    let local_tasks = local
        .get("tasks")
        .and_then(|v| v.as_array())
        .expect("tasks array");
    assert!(
        local_tasks
            .iter()
            .any(|task| { task.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str()) }),
        "expected local graph to include root task"
    );
    let cluster_id = local
        .get("cluster_id")
        .and_then(|v| v.as_str())
        .expect("cluster_id");

    let cluster = http_get_json(
        port,
        &format!("/api/graph/cluster/{cluster_id}?lens=work&limit=200"),
    );
    let cluster_tasks = cluster
        .get("tasks")
        .and_then(|v| v.as_array())
        .expect("tasks array");
    assert!(
        cluster_tasks
            .iter()
            .any(|task| { task.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str()) }),
        "expected cluster page to include root task"
    );

    let _ = proc.kill();
    let _ = proc.wait();
    let _ = std::fs::remove_dir_all(base);
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
