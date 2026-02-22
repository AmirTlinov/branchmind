use bm_storage::{
    AppendCommitRequest, CreateBranchRequest, CreateMergeRecordRequest, DeleteBranchRequest,
    ListBranchesRequest, ListMergeRecordsRequest, ShowCommitRequest, SqliteStore, StoreError,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_storage_dir(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for tests")
        .as_nanos();
    path.push(format!(
        "bm-storage-v3-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temp storage dir must be creatable");
    path
}

#[test]
fn storage_open_is_fail_closed_on_legacy_schema() {
    let dir = temp_storage_dir("legacy-reset-required");
    let db_path = dir.join("branchmind_rust.db");

    let conn = Connection::open(db_path).expect("legacy db must open");
    conn.execute("CREATE TABLE legacy_tasks(id TEXT PRIMARY KEY)", [])
        .expect("legacy table should be created");
    drop(conn);

    let err = SqliteStore::open(&dir).expect_err("legacy storage must be rejected");
    assert_eq!(err.code(), "RESET_REQUIRED");
    assert!(matches!(
        err,
        StoreError::InvalidInput(message) if message.starts_with("RESET_REQUIRED")
    ));
}

#[test]
fn v3_branch_commit_merge_api_and_atomic_merge_write() {
    let dir = temp_storage_dir("merge-atomicity");
    let mut store = SqliteStore::open(&dir).expect("fresh storage should open");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "main".to_string(),
            parent_branch_id: None,
            created_at_ms: 10,
        })
        .expect("main branch should be created");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "feature".to_string(),
            parent_branch_id: Some("main".to_string()),
            created_at_ms: 11,
        })
        .expect("feature branch should be created");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "feature".to_string(),
            commit_id: "c-f-1".to_string(),
            parent_commit_id: None,
            message: "feature init".to_string(),
            body: "feature work".to_string(),
            created_at_ms: 12,
        })
        .expect("feature commit should be appended");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "feature".to_string(),
            commit_id: "c-f-2".to_string(),
            parent_commit_id: None,
            message: "feature follow-up".to_string(),
            body: "feature work 2".to_string(),
            created_at_ms: 12,
        })
        .expect("feature second commit should be appended");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "main".to_string(),
            commit_id: "c-m-1".to_string(),
            parent_commit_id: None,
            message: "main init".to_string(),
            body: "main work".to_string(),
            created_at_ms: 13,
        })
        .expect("main commit should be appended");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "hotfix".to_string(),
            parent_branch_id: Some("main".to_string()),
            created_at_ms: 13,
        })
        .expect("child branch should be created from main");

    let branches_after_child = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-a".to_string(),
            limit: 20,
            offset: 0,
        })
        .expect("branches should list after child create");
    let hotfix_branch = branches_after_child
        .iter()
        .find(|branch| branch.branch_id() == "hotfix")
        .expect("hotfix branch must exist");
    assert_eq!(hotfix_branch.head_commit_id(), Some("c-m-1"));

    let merge = store
        .create_merge_record(CreateMergeRecordRequest {
            workspace_id: "ws-a".to_string(),
            merge_id: "merge-1".to_string(),
            source_branch_id: "feature".to_string(),
            target_branch_id: "main".to_string(),
            strategy: "squash".to_string(),
            summary: "integrate feature".to_string(),
            synthesis_commit_id: "c-m-merge-1".to_string(),
            synthesis_message: "merge feature".to_string(),
            synthesis_body: "synthesis content".to_string(),
            created_at_ms: 14,
        })
        .expect("merge record should be created");
    assert_eq!(merge.synthesis_commit_id(), "c-m-merge-1");

    let merges = store
        .list_merge_records(ListMergeRecordsRequest {
            workspace_id: "ws-a".to_string(),
            limit: 10,
            offset: 0,
        })
        .expect("merge records should list");
    assert_eq!(merges.len(), 1);

    let duplicate_merge_err = store
        .create_merge_record(CreateMergeRecordRequest {
            workspace_id: "ws-a".to_string(),
            merge_id: "merge-1".to_string(),
            source_branch_id: "feature".to_string(),
            target_branch_id: "main".to_string(),
            strategy: "squash".to_string(),
            summary: "duplicate merge id".to_string(),
            synthesis_commit_id: "c-m-merge-2".to_string(),
            synthesis_message: "merge feature second".to_string(),
            synthesis_body: "should rollback".to_string(),
            created_at_ms: 15,
        })
        .expect_err("duplicate merge id should fail and rollback");
    assert_eq!(duplicate_merge_err.code(), "ALREADY_EXISTS");

    let rolled_back_commit = store
        .show_commit(ShowCommitRequest {
            workspace_id: "ws-a".to_string(),
            commit_id: "c-m-merge-2".to_string(),
        })
        .expect("show commit should succeed");
    assert!(
        rolled_back_commit.is_none(),
        "synthesis commit must rollback when merge insert fails"
    );

    let branches = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-a".to_string(),
            limit: 10,
            offset: 0,
        })
        .expect("branches should list");

    let main_branch = branches
        .iter()
        .find(|branch| branch.branch_id() == "main")
        .expect("main branch must exist");
    assert_eq!(main_branch.head_commit_id(), Some("c-m-merge-1"));

    store
        .delete_branch(DeleteBranchRequest {
            workspace_id: "ws-a".to_string(),
            branch_id: "feature".to_string(),
        })
        .expect("feature branch should be deletable");

    let deleted_feature_commit = store
        .show_commit(ShowCommitRequest {
            workspace_id: "ws-a".to_string(),
            commit_id: "c-f-1".to_string(),
        })
        .expect("show commit should succeed after delete");
    assert!(
        deleted_feature_commit.is_none(),
        "feature commits must be removed via foreign key cascade"
    );

    let deleted_feature_commit_2 = store
        .show_commit(ShowCommitRequest {
            workspace_id: "ws-a".to_string(),
            commit_id: "c-f-2".to_string(),
        })
        .expect("show commit should succeed after delete");
    assert!(
        deleted_feature_commit_2.is_none(),
        "feature commit chain must be removed on branch delete"
    );

    let branches_after_delete = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-a".to_string(),
            limit: 10,
            offset: 0,
        })
        .expect("branches should list after delete");
    assert!(
        branches_after_delete
            .iter()
            .all(|branch| branch.branch_id() != "feature")
    );
}

#[test]
fn delete_branch_fails_when_descendants_exist() {
    let dir = temp_storage_dir("delete-branch-descendants");
    let mut store = SqliteStore::open(&dir).expect("fresh storage should open");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-c".to_string(),
            branch_id: "main".to_string(),
            parent_branch_id: None,
            created_at_ms: 10,
        })
        .expect("main branch should be created");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-c".to_string(),
            branch_id: "child".to_string(),
            parent_branch_id: Some("main".to_string()),
            created_at_ms: 11,
        })
        .expect("child branch should be created");

    let err = store
        .delete_branch(DeleteBranchRequest {
            workspace_id: "ws-c".to_string(),
            branch_id: "main".to_string(),
        })
        .expect_err("parent branch delete must fail while descendants exist");
    assert_eq!(err.code(), "INVALID_INPUT");

    let branches = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-c".to_string(),
            limit: 10,
            offset: 0,
        })
        .expect("branches should list");

    assert!(branches.iter().any(|branch| branch.branch_id() == "main"));
    assert!(branches.iter().any(|branch| branch.branch_id() == "child"));
}

#[test]
fn legacy_branch_inserts_also_set_non_null_updated_at_ms() {
    let dir = temp_storage_dir("legacy-branch-updated-at");
    let mut store = SqliteStore::open(&dir).expect("fresh storage should open");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-legacy".to_string(),
            branch_id: "main".to_string(),
            parent_branch_id: None,
            created_at_ms: 10,
        })
        .expect("main branch should be created");
    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-legacy".to_string(),
            branch_id: "child".to_string(),
            parent_branch_id: Some("main".to_string()),
            created_at_ms: 11,
        })
        .expect("child branch should be created");

    let branches = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-legacy".to_string(),
            limit: 20,
            offset: 0,
        })
        .expect("branches should list");

    assert!(
        branches
            .iter()
            .all(|branch| branch.updated_at_ms() >= branch.created_at_ms()),
        "all branch rows must have non-null monotonic updated_at_ms"
    );
}

#[test]
fn branch_updated_at_is_monotonic_for_stale_commit_and_merge_timestamps() {
    let dir = temp_storage_dir("branch-updated-at-monotonic");
    let mut store = SqliteStore::open(&dir).expect("fresh storage should open");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-b".to_string(),
            branch_id: "main".to_string(),
            parent_branch_id: None,
            created_at_ms: 100,
        })
        .expect("main branch should be created");

    store
        .create_branch(CreateBranchRequest {
            workspace_id: "ws-b".to_string(),
            branch_id: "feature".to_string(),
            parent_branch_id: Some("main".to_string()),
            created_at_ms: 101,
        })
        .expect("feature branch should be created");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-b".to_string(),
            branch_id: "main".to_string(),
            commit_id: "c-main-1".to_string(),
            parent_commit_id: None,
            message: "first main".to_string(),
            body: "first main body".to_string(),
            created_at_ms: 200,
        })
        .expect("first main commit should be appended");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-b".to_string(),
            branch_id: "main".to_string(),
            commit_id: "c-main-stale".to_string(),
            parent_commit_id: None,
            message: "stale main".to_string(),
            body: "stale body".to_string(),
            created_at_ms: 150,
        })
        .expect("stale main commit should be accepted with clamped updated_at_ms");

    store
        .append_commit(AppendCommitRequest {
            workspace_id: "ws-b".to_string(),
            branch_id: "feature".to_string(),
            commit_id: "c-feature-1".to_string(),
            parent_commit_id: None,
            message: "feature init".to_string(),
            body: "feature body".to_string(),
            created_at_ms: 220,
        })
        .expect("feature commit should be appended");

    store
        .create_merge_record(CreateMergeRecordRequest {
            workspace_id: "ws-b".to_string(),
            merge_id: "merge-stale-time".to_string(),
            source_branch_id: "feature".to_string(),
            target_branch_id: "main".to_string(),
            strategy: "squash".to_string(),
            summary: "merge with stale timestamp".to_string(),
            synthesis_commit_id: "c-main-merge-stale".to_string(),
            synthesis_message: "merge stale".to_string(),
            synthesis_body: "merge stale body".to_string(),
            created_at_ms: 180,
        })
        .expect("merge should succeed with clamped updated_at_ms");

    let branches = store
        .list_branches(ListBranchesRequest {
            workspace_id: "ws-b".to_string(),
            limit: 10,
            offset: 0,
        })
        .expect("branches should list");

    let main_branch = branches
        .iter()
        .find(|branch| branch.branch_id() == "main")
        .expect("main branch must exist");

    assert_eq!(main_branch.updated_at_ms(), 200);
    assert_eq!(main_branch.head_commit_id(), Some("c-main-merge-stale"));
}
