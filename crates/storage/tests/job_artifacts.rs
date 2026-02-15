#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::{JobArtifactCreateRequest, JobArtifactGetRequest, JobCreateRequest, SqliteStore};
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

fn setup(test_name: &str) -> (SqliteStore, WorkspaceId) {
    let dir = temp_dir(test_name);
    let mut store = SqliteStore::open(&dir).expect("open store");
    let ws = WorkspaceId::try_new("test-ws".to_string()).expect("ws id");
    store.workspace_init(&ws).expect("init workspace");
    (store, ws)
}

fn create_job(store: &mut SqliteStore, ws: &WorkspaceId) -> String {
    let result = store
        .job_create(
            ws,
            JobCreateRequest {
                title: "Test job".to_string(),
                prompt: "Do something".to_string(),
                kind: "test".to_string(),
                priority: "MEDIUM".to_string(),
                task_id: None,
                anchor_id: None,
                meta_json: None,
            },
        )
        .expect("create job");
    result.job.id
}

#[test]
fn create_and_get_artifact() {
    let (mut store, ws) = setup("create_and_get");
    let job_id = create_job(&mut store, &ws);

    let content = "Hello, this is test content for the artifact.";
    let artifact = store
        .job_artifact_create(
            &ws,
            JobArtifactCreateRequest {
                job_id: job_id.clone(),
                artifact_key: "scout_context_rendered".to_string(),
                content_text: content.to_string(),
            },
        )
        .expect("create artifact");

    assert_eq!(artifact.job_id, job_id);
    assert_eq!(artifact.artifact_key, "scout_context_rendered");
    assert_eq!(artifact.content_text, content);
    assert_eq!(artifact.content_len, content.len() as i64);

    let fetched = store
        .job_artifact_get(
            &ws,
            JobArtifactGetRequest {
                job_id: job_id.clone(),
                artifact_key: "scout_context_rendered".to_string(),
            },
        )
        .expect("get artifact");

    let fetched = fetched.expect("artifact should exist");
    assert_eq!(fetched.content_text, content);
    assert_eq!(fetched.content_len, content.len() as i64);
}

#[test]
fn list_artifacts_is_limited_and_sorted() {
    let (mut store, ws) = setup("list_artifacts");
    let job_id = create_job(&mut store, &ws);

    for key in ["key_b", "key_a", "key_c"] {
        store
            .job_artifact_create(
                &ws,
                JobArtifactCreateRequest {
                    job_id: job_id.clone(),
                    artifact_key: key.to_string(),
                    content_text: format!("content-{key}"),
                },
            )
            .expect("create artifact");
    }

    let list = store
        .job_artifacts_list(
            &ws,
            bm_storage::JobArtifactsListRequest {
                job_id: job_id.clone(),
                limit: 2,
            },
        )
        .expect("list artifacts");

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].artifact_key, "key_a");
    assert_eq!(list[1].artifact_key, "key_b");
}

#[test]
fn get_nonexistent_artifact_returns_none() {
    let (mut store, ws) = setup("get_nonexistent");
    let job_id = create_job(&mut store, &ws);

    let result = store
        .job_artifact_get(
            &ws,
            JobArtifactGetRequest {
                job_id,
                artifact_key: "nope".to_string(),
            },
        )
        .expect("get artifact");

    assert!(result.is_none());
}

#[test]
fn artifact_exceeds_max_len() {
    let (mut store, ws) = setup("exceeds_max_len");
    let job_id = create_job(&mut store, &ws);

    let big_content = "x".repeat(512_001);
    let result = store.job_artifact_create(
        &ws,
        JobArtifactCreateRequest {
            job_id,
            artifact_key: "big".to_string(),
            content_text: big_content,
        },
    );

    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("max length"), "error: {err}");
}

#[test]
fn artifact_at_max_len_succeeds() {
    let (mut store, ws) = setup("at_max_len");
    let job_id = create_job(&mut store, &ws);

    let content = "x".repeat(512_000);
    let artifact = store
        .job_artifact_create(
            &ws,
            JobArtifactCreateRequest {
                job_id,
                artifact_key: "max".to_string(),
                content_text: content.clone(),
            },
        )
        .expect("create artifact at max len");

    assert_eq!(artifact.content_len, 512_000);
}

#[test]
fn max_artifacts_per_job() {
    let (mut store, ws) = setup("max_artifacts");
    let job_id = create_job(&mut store, &ws);

    // Create 8 artifacts (the max).
    for i in 0..8 {
        store
            .job_artifact_create(
                &ws,
                JobArtifactCreateRequest {
                    job_id: job_id.clone(),
                    artifact_key: format!("key_{i}"),
                    content_text: format!("content {i}"),
                },
            )
            .unwrap_or_else(|e| panic!("create artifact {i}: {e}"));
    }

    // 9th artifact with a new key should fail.
    let result = store.job_artifact_create(
        &ws,
        JobArtifactCreateRequest {
            job_id: job_id.clone(),
            artifact_key: "key_overflow".to_string(),
            content_text: "overflow".to_string(),
        },
    );
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("max artifacts"), "error: {err}");
}

#[test]
fn upsert_existing_key_doesnt_hit_limit() {
    let (mut store, ws) = setup("upsert_existing");
    let job_id = create_job(&mut store, &ws);

    // Create 8 artifacts.
    for i in 0..8 {
        store
            .job_artifact_create(
                &ws,
                JobArtifactCreateRequest {
                    job_id: job_id.clone(),
                    artifact_key: format!("key_{i}"),
                    content_text: format!("content {i}"),
                },
            )
            .unwrap_or_else(|e| panic!("create artifact {i}: {e}"));
    }

    // Upsert an existing key — should succeed (not a new key).
    let artifact = store
        .job_artifact_create(
            &ws,
            JobArtifactCreateRequest {
                job_id: job_id.clone(),
                artifact_key: "key_0".to_string(),
                content_text: "updated content".to_string(),
            },
        )
        .expect("upsert existing key");

    assert_eq!(artifact.content_text, "updated content");

    // Verify updated content.
    let fetched = store
        .job_artifact_get(
            &ws,
            JobArtifactGetRequest {
                job_id,
                artifact_key: "key_0".to_string(),
            },
        )
        .expect("get artifact")
        .expect("should exist");

    assert_eq!(fetched.content_text, "updated content");
}

#[test]
fn artifact_for_nonexistent_job() {
    let (mut store, ws) = setup("nonexistent_job");

    let result = store.job_artifact_create(
        &ws,
        JobArtifactCreateRequest {
            job_id: "JOB-999".to_string(),
            artifact_key: "test".to_string(),
            content_text: "content".to_string(),
        },
    );

    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("unknown id"), "error: {err}");
}
