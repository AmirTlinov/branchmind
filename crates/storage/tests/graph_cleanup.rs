#![forbid(unsafe_code)]

use bm_core::graph::GraphQueryRequest;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{NewStep, SqliteStore, TaskCreateRequest};
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
fn graph_cleanup_on_step_and_task_delete() {
    let storage_dir = temp_dir("graph_cleanup_on_step_and_task_delete");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws_graph_cleanup").expect("workspace id");

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
                event_payload_json: "{}".to_string(),
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
                event_payload_json: "{}".to_string(),
            },
        )
        .expect("create task");

    let decomposed = store
        .steps_decompose(
            &workspace,
            &task_id,
            None,
            None,
            vec![NewStep {
                title: "Step 1".to_string(),
                success_criteria: Vec::new(),
            }],
        )
        .expect("decompose steps");
    let step_ref = decomposed.steps.first().expect("step ref");

    let reasoning = store
        .ensure_reasoning_ref(&workspace, &task_id, TaskKind::Task)
        .expect("reasoning ref");
    let branch = reasoning.branch.clone();
    let graph_doc = reasoning.graph_doc.clone();

    let step_node_id = format!("step:{}", step_ref.step_id);
    let query = store
        .graph_query(
            &workspace,
            &branch,
            &graph_doc,
            GraphQueryRequest {
                ids: Some(vec![step_node_id.clone()]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 20,
                include_edges: true,
                edges_limit: 50,
            },
        )
        .expect("graph query");
    assert_eq!(query.nodes.len(), 1, "step node must exist");

    store
        .step_delete(
            &workspace,
            &task_id,
            None,
            Some(&step_ref.step_id),
            None,
            false,
        )
        .expect("step delete");

    let validate = store
        .graph_validate(&workspace, &branch, &graph_doc, 10)
        .expect("graph validate");
    assert!(
        validate.ok,
        "graph validate should pass after step delete: {:?}",
        validate.errors
    );

    let query_after = store
        .graph_query(
            &workspace,
            &branch,
            &graph_doc,
            GraphQueryRequest {
                ids: Some(vec![step_node_id]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 20,
                include_edges: true,
                edges_limit: 50,
            },
        )
        .expect("graph query after delete");
    assert_eq!(query_after.nodes.len(), 1, "step tombstone should remain");
    assert!(
        query_after.nodes[0].deleted,
        "step node should be marked deleted"
    );

    store
        .task_root_delete(&workspace, &task_id, false)
        .expect("task delete");

    let validate_task = store
        .graph_validate(&workspace, &branch, &graph_doc, 10)
        .expect("graph validate after task delete");
    assert!(
        validate_task.ok,
        "graph validate should pass after task delete"
    );

    let task_node_id = format!("task:{task_id}");
    let query_task = store
        .graph_query(
            &workspace,
            &branch,
            &graph_doc,
            GraphQueryRequest {
                ids: Some(vec![task_node_id]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 20,
                include_edges: true,
                edges_limit: 50,
            },
        )
        .expect("graph query task");
    assert_eq!(query_task.nodes.len(), 1, "task tombstone should remain");
    assert!(
        query_task.nodes[0].deleted,
        "task node should be marked deleted"
    );
}
