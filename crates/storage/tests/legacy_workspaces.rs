#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::SqliteStore;
use rusqlite::Connection;
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
fn list_workspaces_supports_legacy_schema_without_project_guard() {
    let storage_dir = temp_dir("legacy_schema_without_project_guard");
    let db_path = storage_dir.join("branchmind_rust.db");

    // Simulate an older store schema where `workspaces.project_guard` didn't exist yet.
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute_batch(
        "CREATE TABLE workspaces(workspace TEXT PRIMARY KEY, created_at_ms INTEGER NOT NULL);\n\
         INSERT INTO workspaces(workspace, created_at_ms) VALUES ('ws1', 1);",
    )
    .expect("seed legacy schema");
    drop(conn);

    let store = SqliteStore::open_read_only(&storage_dir).expect("open read-only store");
    let rows = store.list_workspaces(10, 0).expect("list workspaces");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].workspace, "ws1");
    assert_eq!(rows[0].project_guard, None);

    // Also ensure project guard getter doesn't explode on legacy schemas.
    let ws = WorkspaceId::try_new("ws1").expect("workspace id");
    let guard = store
        .workspace_project_guard_get(&ws)
        .expect("project guard get");
    assert_eq!(guard, None);
}
