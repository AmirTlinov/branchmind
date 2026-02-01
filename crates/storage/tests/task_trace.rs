#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{
    DocEntriesSinceRequest, DocEntryKind, NewStep, SqliteStore, StepNoteRequest, StepSelector,
    TaskCreateRequest,
};
use std::collections::HashSet;
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

fn trace_event_ids(
    store: &SqliteStore,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
) -> HashSet<String> {
    store
        .doc_entries_since(
            workspace,
            DocEntriesSinceRequest {
                branch: branch.to_string(),
                doc: doc.to_string(),
                since_seq: 0,
                limit: 200,
                kind: Some(DocEntryKind::Event),
            },
        )
        .expect("trace entries")
        .entries
        .into_iter()
        .filter_map(|entry| entry.source_event_id)
        .collect()
}

#[test]
fn task_mutations_emit_trace_and_notes() {
    let storage_dir = temp_dir("task_mutations_emit_trace_and_notes");
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
                parent_plan_id: Some(plan_id.clone()),
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

    let note_text = "progress note";
    store
        .step_note(
            &workspace,
            StepNoteRequest {
                task_id: task_id.clone(),
                expected_revision: None,
                agent_id: None,
                selector: StepSelector {
                    step_id: Some(step_ref.step_id.clone()),
                    path: None,
                },
                note: note_text.to_string(),
            },
        )
        .expect("step note");

    let plan_reasoning = store
        .ensure_reasoning_ref(&workspace, &plan_id, TaskKind::Plan)
        .expect("plan reasoning ref");
    let plan_trace_ids = trace_event_ids(
        &store,
        &workspace,
        &plan_reasoning.branch,
        &plan_reasoning.trace_doc,
    );
    let plan_events = store
        .list_events_for_task(&workspace, &plan_id, 50)
        .expect("plan events");
    assert!(!plan_events.is_empty(), "plan should emit events");
    for event in plan_events {
        assert!(
            plan_trace_ids.contains(&event.event_id()),
            "missing trace entry for plan event {}",
            event.event_type
        );
    }

    let task_reasoning = store
        .ensure_reasoning_ref(&workspace, &task_id, TaskKind::Task)
        .expect("task reasoning ref");
    let task_trace_ids = trace_event_ids(
        &store,
        &workspace,
        &task_reasoning.branch,
        &task_reasoning.trace_doc,
    );
    let task_events = store
        .list_events_for_task(&workspace, &task_id, 200)
        .expect("task events");
    assert!(!task_events.is_empty(), "task should emit events");
    for event in task_events {
        assert!(
            task_trace_ids.contains(&event.event_id()),
            "missing trace entry for task event {}",
            event.event_type
        );
    }

    let note_entries = store
        .doc_entries_since(
            &workspace,
            DocEntriesSinceRequest {
                branch: task_reasoning.branch,
                doc: task_reasoning.notes_doc,
                since_seq: 0,
                limit: 200,
                kind: Some(DocEntryKind::Note),
            },
        )
        .expect("notes entries")
        .entries;
    assert!(
        note_entries
            .iter()
            .any(|entry| entry.content.as_deref() == Some(note_text)),
        "note should be mirrored into notes_doc"
    );
}
