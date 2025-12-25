#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::{EventRow, SqliteStore};
use std::path::PathBuf;

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_storage_{test_name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn doc_ingest_task_event_is_idempotent() {
    let storage_dir = temp_dir("doc_ingest_task_event_is_idempotent");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws1").expect("workspace id");
    let event = EventRow {
        seq: 1,
        ts_ms: 123,
        task_id: Some("TASK-001".to_string()),
        path: Some("s:0".to_string()),
        event_type: "task_created".to_string(),
        payload_json: "{}".to_string(),
    };

    let inserted = store
        .doc_ingest_task_event(&workspace, "branch-1", "doc-1", &event)
        .expect("ingest event");
    assert!(inserted);

    let inserted_again = store
        .doc_ingest_task_event(&workspace, "branch-1", "doc-1", &event)
        .expect("ingest event again");
    assert!(!inserted_again);
}
