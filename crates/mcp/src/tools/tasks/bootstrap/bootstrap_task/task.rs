#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::args::BootstrapStepInput;

pub(super) struct CreatedTask {
    pub id: String,
    pub revision: i64,
    pub steps: Vec<bm_storage::StepRef>,
}

pub(super) fn create_task_with_steps(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    parent_plan_id: &str,
    task_title: String,
    description: Option<String>,
    steps: Vec<BootstrapStepInput>,
    agent_id: Option<String>,
    events: &mut Vec<Value>,
) -> Result<CreatedTask, Value> {
    let payload = json!({
        "kind": "task",
        "title": task_title,
        "parent": parent_plan_id
    })
    .to_string();
    let (task_id, _revision, create_event) = match server.store.create(
        workspace,
        bm_storage::TaskCreateRequest {
            kind: TaskKind::Task,
            title: task_title,
            parent_plan_id: Some(parent_plan_id.to_string()),
            description,
            contract: None,
            contract_json: None,
            event_type: "task_created".to_string(),
            event_payload_json: payload,
        },
    ) {
        Ok(v) => v,
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    events.push(events_to_json(vec![create_event]).remove(0));

    let decompose_steps = steps
        .iter()
        .map(|step| bm_storage::NewStep {
            title: step.title.clone(),
            success_criteria: step.criteria.clone(),
        })
        .collect::<Vec<_>>();
    let decompose =
        match server
            .store
            .steps_decompose(workspace, &task_id, None, None, decompose_steps)
        {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
    events.push(events_to_json(vec![decompose.event]).remove(0));

    let mut revision = decompose.task_revision;
    for (step, step_ref) in steps.iter().zip(decompose.steps.iter()) {
        let defined = match server.store.step_define(
            workspace,
            bm_storage::StepDefineRequest {
                task_id: task_id.clone(),
                expected_revision: None,
                agent_id: agent_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: Some(step_ref.step_id.clone()),
                    path: None,
                },
                patch: bm_storage::StepPatch {
                    title: None,
                    success_criteria: None,
                    tests: Some(step.tests.clone()),
                    blockers: Some(step.blockers.clone()),
                    proof_tests_mode: (step.proof_tests_mode != bm_storage::ProofMode::Off)
                        .then_some(step.proof_tests_mode),
                    proof_security_mode: (step.proof_security_mode != bm_storage::ProofMode::Off)
                        .then_some(step.proof_security_mode),
                    proof_perf_mode: (step.proof_perf_mode != bm_storage::ProofMode::Off)
                        .then_some(step.proof_perf_mode),
                    proof_docs_mode: (step.proof_docs_mode != bm_storage::ProofMode::Off)
                        .then_some(step.proof_docs_mode),
                },
            },
        ) {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        revision = defined.task_revision;
        events.push(events_to_json(vec![defined.event]).remove(0));
    }

    Ok(CreatedTask {
        id: task_id,
        revision,
        steps: decompose.steps,
    })
}
