#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::{
    AnchorUpsertRequest, AnchorsBootstrapRequest, AnchorsListRequest, SqliteStore, StoreError,
};
use rusqlite::{Connection, params};
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
fn anchors_bootstrap_rolls_back_on_alias_collision() {
    let storage_dir = temp_dir("anchors_bootstrap_rolls_back_on_alias_collision");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws_atomic").expect("workspace id");
    store.workspace_init(&workspace).expect("workspace init");

    let request = AnchorsBootstrapRequest {
        anchors: vec![
            AnchorUpsertRequest {
                id: "a:alpha".to_string(),
                title: "Alpha".to_string(),
                kind: "ops".to_string(),
                description: None,
                refs: Vec::new(),
                aliases: Vec::new(),
                parent_id: None,
                depends_on: Vec::new(),
                status: "active".to_string(),
            },
            AnchorUpsertRequest {
                id: "a:beta".to_string(),
                title: "Beta".to_string(),
                kind: "ops".to_string(),
                description: None,
                refs: Vec::new(),
                aliases: vec!["a:alpha".to_string()],
                parent_id: None,
                depends_on: Vec::new(),
                status: "active".to_string(),
            },
        ],
    };

    let err = store
        .anchors_bootstrap(&workspace, request)
        .expect_err("expected alias collision to fail");
    match err {
        StoreError::InvalidInput(msg) => {
            assert_eq!(msg, "anchor.aliases must not include an existing anchor id");
        }
        other => panic!("expected InvalidInput error, got {other:?}"),
    }

    let listed = store
        .anchors_list(
            &workspace,
            AnchorsListRequest {
                text: None,
                kind: None,
                status: None,
                limit: 10,
            },
        )
        .expect("anchors list");
    assert!(
        listed.anchors.is_empty(),
        "expected atomic rollback, found {} anchors",
        listed.anchors.len()
    );
}

#[test]
fn uncommitted_transaction_is_not_persisted_after_reopen() {
    let storage_dir = temp_dir("uncommitted_transaction_is_not_persisted_after_reopen");
    let workspace = WorkspaceId::try_new("ws_crash").expect("workspace id");

    {
        let _store = SqliteStore::open(&storage_dir).expect("open store");
    }

    let db_path = storage_dir.join("branchmind_rust.db");
    {
        let mut conn = Connection::open(&db_path).expect("open db");
        let tx = conn.transaction().expect("begin tx");
        tx.execute(
            "INSERT INTO workspaces (workspace, created_at_ms, project_guard) VALUES (?1, ?2, ?3)",
            params![workspace.as_str(), 0i64, Option::<String>::None],
        )
        .expect("insert workspace");
        // Drop without commit -> rollback (simulated crash before commit).
    }

    let store = SqliteStore::open(&storage_dir).expect("open store again");
    let exists = store
        .workspace_exists(&workspace)
        .expect("workspace exists");
    assert!(!exists, "uncommitted transaction should not persist");
}
