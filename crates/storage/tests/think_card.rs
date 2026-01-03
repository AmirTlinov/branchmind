#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{
    GraphQueryRequest, SqliteStore, StoreError, TaskCreateRequest, ThinkCardCommitRequest,
    ThinkCardInput,
};
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
fn think_card_commit_is_atomic_and_idempotent() {
    let storage_dir = temp_dir("think_card_commit_is_atomic_and_idempotent");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws1").expect("workspace id");

    let (plan_id, _, _) = store
        .create(
            &workspace,
            TaskCreateRequest {
                kind: TaskKind::Plan,
                title: "Plan".to_string(),
                parent_plan_id: None,
                description: None,
                contract: None,
                contract_json: None,
                event_type: "plan_created".to_string(),
                event_payload_json: r#"{"title":"Plan"}"#.to_string(),
            },
        )
        .expect("create plan");
    let (task_id, _, _) = store
        .create(
            &workspace,
            TaskCreateRequest {
                kind: TaskKind::Task,
                title: "Task".to_string(),
                parent_plan_id: Some(plan_id),
                description: None,
                contract: None,
                contract_json: None,
                event_type: "task_created".to_string(),
                event_payload_json: r#"{"title":"Task"}"#.to_string(),
            },
        )
        .expect("create task");

    let reasoning = store
        .ensure_reasoning_ref(&workspace, &task_id, TaskKind::Task)
        .expect("ensure reasoning ref");

    let card2 = ThinkCardInput {
        card_id: "CARD-2".to_string(),
        card_type: "note".to_string(),
        title: Some("Second".to_string()),
        text: Some("Second text".to_string()),
        status: Some("open".to_string()),
        tags: vec![],
        meta_json: None,
        content: "Second text".to_string(),
        payload_json: r#"{"id":"CARD-2","type":"note","title":"Second","text":"Second text","status":"open","tags":[]}"#
            .to_string(),
    };
    let out2 = store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: reasoning.branch.clone(),
                trace_doc: reasoning.trace_doc.clone(),
                graph_doc: reasoning.graph_doc.clone(),
                card: card2,
                supports: vec![],
                blocks: vec![],
            },
        )
        .expect("commit card2");
    assert!(out2.inserted);
    assert_eq!(out2.nodes_upserted, 1);
    assert_eq!(out2.edges_upserted, 0);

    let card1 = ThinkCardInput {
        card_id: "CARD-1".to_string(),
        card_type: "hypothesis".to_string(),
        title: Some("Hypothesis".to_string()),
        text: Some("It works".to_string()),
        status: Some("open".to_string()),
        tags: vec!["ux".to_string(), "mvp".to_string()],
        meta_json: None,
        content: "It works".to_string(),
        payload_json: r#"{"id":"CARD-1","type":"hypothesis","title":"Hypothesis","text":"It works","status":"open","tags":["mvp","ux"]}"#
            .to_string(),
    };

    let out1 = store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: reasoning.branch.clone(),
                trace_doc: reasoning.trace_doc.clone(),
                graph_doc: reasoning.graph_doc.clone(),
                card: card1,
                supports: vec!["CARD-2".to_string()],
                blocks: vec![],
            },
        )
        .expect("commit card1");
    assert!(out1.inserted);
    assert_eq!(out1.nodes_upserted, 1);
    assert_eq!(out1.edges_upserted, 1);

    // Second commit of the same payload must be a no-op (no duplicate trace entry, no graph noise).
    let out1_again = store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: reasoning.branch.clone(),
                trace_doc: reasoning.trace_doc.clone(),
                graph_doc: reasoning.graph_doc.clone(),
                card: ThinkCardInput {
                    card_id: "CARD-1".to_string(),
                    card_type: "hypothesis".to_string(),
                    title: Some("Hypothesis".to_string()),
                    text: Some("It works".to_string()),
                    status: Some("open".to_string()),
                    tags: vec!["ux".to_string(), "mvp".to_string()],
                    meta_json: None,
                    content: "It works".to_string(),
                    payload_json: r#"{"id":"CARD-1","type":"hypothesis","title":"Hypothesis","text":"It works","status":"open","tags":["mvp","ux"]}"#
                        .to_string(),
                },
                supports: vec!["CARD-2".to_string()],
                blocks: vec![],
            },
        )
        .expect("commit card1 again");
    assert!(!out1_again.inserted);
    assert_eq!(out1_again.nodes_upserted, 0);
    assert_eq!(out1_again.edges_upserted, 0);

    // Trace contains exactly two notes (CARD-2 + CARD-1), regardless of repeats.
    let trace = store
        .doc_show_tail(
            &workspace,
            &reasoning.branch,
            &reasoning.trace_doc,
            None,
            50,
        )
        .expect("show trace");
    let notes = trace
        .entries
        .iter()
        .filter(|e| e.kind.as_str() == "note")
        .count();
    assert_eq!(notes, 2);

    // Graph contains both nodes and the support edge between them.
    let graph = store
        .graph_query(
            &workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            GraphQueryRequest {
                ids: Some(vec!["CARD-1".to_string(), "CARD-2".to_string()]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 10,
                include_edges: true,
                edges_limit: 50,
            },
        )
        .expect("graph query");
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.id == "CARD-1" && n.node_type == "hypothesis"),
        "graph must include CARD-1 node"
    );
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.id == "CARD-2" && n.node_type == "note"),
        "graph must include CARD-2 node"
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.from == "CARD-1" && e.rel == "supports" && e.to == "CARD-2"),
        "graph must include supports edge CARD-1 -> CARD-2"
    );
}

#[test]
fn think_card_commit_rejects_payload_mismatch_for_same_id() {
    let storage_dir = temp_dir("think_card_commit_rejects_payload_mismatch_for_same_id");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws1").expect("workspace id");

    let (plan_id, _, _) = store
        .create(
            &workspace,
            TaskCreateRequest {
                kind: TaskKind::Plan,
                title: "Plan".to_string(),
                parent_plan_id: None,
                description: None,
                contract: None,
                contract_json: None,
                event_type: "plan_created".to_string(),
                event_payload_json: r#"{"title":"Plan"}"#.to_string(),
            },
        )
        .expect("create plan");
    let (task_id, _, _) = store
        .create(
            &workspace,
            TaskCreateRequest {
                kind: TaskKind::Task,
                title: "Task".to_string(),
                parent_plan_id: Some(plan_id),
                description: None,
                contract: None,
                contract_json: None,
                event_type: "task_created".to_string(),
                event_payload_json: r#"{"title":"Task"}"#.to_string(),
            },
        )
        .expect("create task");

    let reasoning = store
        .ensure_reasoning_ref(&workspace, &task_id, TaskKind::Task)
        .expect("ensure reasoning ref");

    store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: reasoning.branch.clone(),
                trace_doc: reasoning.trace_doc.clone(),
                graph_doc: reasoning.graph_doc.clone(),
                card: ThinkCardInput {
                    card_id: "CARD-1".to_string(),
                    card_type: "note".to_string(),
                    title: Some("A".to_string()),
                    text: Some("B".to_string()),
                    status: Some("open".to_string()),
                    tags: vec![],
                    meta_json: None,
                    content: "B".to_string(),
                    payload_json: r#"{"id":"CARD-1","type":"note","title":"A","text":"B","status":"open","tags":[]}"#
                        .to_string(),
                },
                supports: vec![],
                blocks: vec![],
            },
        )
        .expect("first commit");

    let err = store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: reasoning.branch.clone(),
                trace_doc: reasoning.trace_doc.clone(),
                graph_doc: reasoning.graph_doc.clone(),
                card: ThinkCardInput {
                    card_id: "CARD-1".to_string(),
                    card_type: "note".to_string(),
                    title: Some("A".to_string()),
                    text: Some("CHANGED".to_string()),
                    status: Some("open".to_string()),
                    tags: vec![],
                    meta_json: None,
                    content: "CHANGED".to_string(),
                    payload_json: r#"{"id":"CARD-1","type":"note","title":"A","text":"CHANGED","status":"open","tags":[]}"#
                        .to_string(),
                },
                supports: vec![],
                blocks: vec![],
            },
        )
        .expect_err("expected payload mismatch error");

    match err {
        StoreError::InvalidInput(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}
