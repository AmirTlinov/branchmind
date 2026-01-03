#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{SqliteStore, StoreError};

#[derive(Clone, Debug)]
pub(crate) struct HandoffCore {
    pub(crate) done: Vec<String>,
    pub(crate) remaining: Vec<String>,
    pub(crate) risks: Vec<String>,
}

pub(crate) fn build_handoff_core(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    target_id: &str,
    kind: TaskKind,
) -> Result<HandoffCore, StoreError> {
    match kind {
        TaskKind::Plan => {
            let checklist = store.plan_checklist_get(workspace, target_id)?;
            let total = checklist.steps.len() as i64;
            let done_count = checklist.current.min(total).max(0);

            let mut remaining = Vec::new();
            if total == 0 {
                remaining.push("Checklist is empty".to_string());
            } else if (checklist.current as usize) < checklist.steps.len() {
                remaining.push(format!(
                    "Next checklist item: {}",
                    checklist.steps[checklist.current as usize]
                ));
            }

            Ok(HandoffCore {
                done: vec![format!("Checklist progress: {done_count}/{total}")],
                remaining,
                risks: Vec::new(),
            })
        }
        TaskKind::Task => {
            let summary = store.task_steps_summary(workspace, target_id)?;
            let mut remaining = Vec::new();
            if summary.total_steps == 0 {
                remaining.push("No steps defined".to_string());
            } else {
                remaining.push(format!("Open steps: {}", summary.open_steps));
                if let Some(first) = summary.first_open {
                    remaining.push(format!("Next open step: {}", first.path));
                }
            }

            let mut risks = Vec::new();
            if summary.missing_criteria > 0 {
                risks.push(format!(
                    "Missing criteria checkpoints: {}",
                    summary.missing_criteria
                ));
            }
            if summary.missing_tests > 0 {
                risks.push(format!(
                    "Missing tests checkpoints: {}",
                    summary.missing_tests
                ));
            }
            if summary.missing_security > 0 {
                risks.push(format!(
                    "Missing security checkpoints: {}",
                    summary.missing_security
                ));
            }
            if summary.missing_perf > 0 {
                risks.push(format!(
                    "Missing perf checkpoints: {}",
                    summary.missing_perf
                ));
            }
            if summary.missing_docs > 0 {
                risks.push(format!(
                    "Missing docs checkpoints: {}",
                    summary.missing_docs
                ));
            }
            if summary.missing_proof_tests > 0 {
                risks.push(format!(
                    "Missing required proofs (tests): {}",
                    summary.missing_proof_tests
                ));
            }
            if summary.missing_proof_security > 0 {
                risks.push(format!(
                    "Missing required proofs (security): {}",
                    summary.missing_proof_security
                ));
            }
            if summary.missing_proof_perf > 0 {
                risks.push(format!(
                    "Missing required proofs (perf): {}",
                    summary.missing_proof_perf
                ));
            }
            if summary.missing_proof_docs > 0 {
                risks.push(format!(
                    "Missing required proofs (docs): {}",
                    summary.missing_proof_docs
                ));
            }
            if let Ok(blockers) = store.task_open_blockers(workspace, target_id, 10)
                && !blockers.is_empty()
            {
                risks.push(format!("Open blockers: {}", blockers.len()));
            }

            Ok(HandoffCore {
                done: vec![format!(
                    "Completed steps: {}/{}",
                    summary.completed_steps, summary.total_steps
                )],
                remaining,
                risks,
            })
        }
    }
}
