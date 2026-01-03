#![forbid(unsafe_code)]

use super::json::parse_json_or_null;
use super::reasoning::resolve_reasoning_ref_for_read;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::{SqliteStore, StoreError};
use serde_json::{Value, json};

pub(crate) struct RadarContext {
    pub(crate) target: Value,
    pub(crate) reasoning_ref: Value,
    pub(crate) radar: Value,
    pub(crate) steps: Option<Value>,
}

pub(crate) fn build_radar_context_with_options(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    target_id: &str,
    kind: TaskKind,
    read_only: bool,
) -> Result<RadarContext, StoreError> {
    let target = match kind {
        TaskKind::Plan => match store.get_plan(workspace, target_id)? {
            Some(p) => json!({
                "id": p.id,
                "qualified_id": format!("{}:{}", workspace.as_str(), p.id),
                "kind": "plan",
                "revision": p.revision,
                "status": p.status,
                "title": p.title,
                "contract": p.contract,
                "contract_data": parse_json_or_null(p.contract_json),
                "created_at_ms": p.created_at_ms,
                "updated_at_ms": p.updated_at_ms
            }),
            None => return Err(StoreError::UnknownId),
        },
        TaskKind::Task => match store.get_task(workspace, target_id)? {
            Some(t) => json!({
                "id": t.id,
                "qualified_id": format!("{}:{}", workspace.as_str(), t.id),
                "kind": "task",
                "revision": t.revision,
                "status": t.status,
                "parent": t.parent_plan_id,
                "title": t.title,
                "description": t.description,
                "created_at_ms": t.created_at_ms,
                "updated_at_ms": t.updated_at_ms
            }),
            None => return Err(StoreError::UnknownId),
        },
    };

    let (reasoning_ref, _existing) =
        resolve_reasoning_ref_for_read(store, workspace, target_id, kind, read_only)?;
    let reasoning_ref_json = json!({
        "branch": reasoning_ref.branch,
        "notes_doc": reasoning_ref.notes_doc,
        "graph_doc": reasoning_ref.graph_doc,
        "trace_doc": reasoning_ref.trace_doc
    });

    let now = match kind {
        TaskKind::Plan => format!(
            "Plan {}: {}",
            target_id,
            target.get("title").and_then(|v| v.as_str()).unwrap_or("")
        ),
        TaskKind::Task => format!(
            "Task {}: {}",
            target_id,
            target.get("title").and_then(|v| v.as_str()).unwrap_or("")
        ),
    };

    let why = match kind {
        TaskKind::Plan => target
            .get("contract")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        TaskKind::Task => target
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };

    let mut verify = Vec::<String>::new();
    let mut next = Vec::<String>::new();
    let mut blockers = Vec::<String>::new();
    let mut steps_summary: Option<Value> = None;

    if kind == TaskKind::Task {
        if let Ok(summary) = store.task_steps_summary(workspace, target_id) {
            steps_summary = Some(json!({
                "total": summary.total_steps,
                "open": summary.open_steps,
                "completed": summary.completed_steps,
                "missing_criteria": summary.missing_criteria,
                "missing_tests": summary.missing_tests,
                "missing_security": summary.missing_security,
                "missing_perf": summary.missing_perf,
                "missing_docs": summary.missing_docs,
                "missing_proof_tests": summary.missing_proof_tests,
                "missing_proof_security": summary.missing_proof_security,
                "missing_proof_perf": summary.missing_proof_perf,
                "missing_proof_docs": summary.missing_proof_docs,
                "first_open": summary.first_open.as_ref().map(|s| json!({
                    "step_id": s.step_id,
                    "path": s.path,
                    "title": s.title,
                    "criteria_confirmed": s.criteria_confirmed,
                    "tests_confirmed": s.tests_confirmed,
                    "security_confirmed": s.security_confirmed,
                    "perf_confirmed": s.perf_confirmed,
                    "docs_confirmed": s.docs_confirmed,
                    "require_security": s.require_security,
                    "require_perf": s.require_perf,
                    "require_docs": s.require_docs,
                    "proof_tests_mode": s.proof_tests_mode.as_str(),
                    "proof_security_mode": s.proof_security_mode.as_str(),
                    "proof_perf_mode": s.proof_perf_mode.as_str(),
                    "proof_docs_mode": s.proof_docs_mode.as_str(),
                    "proof_tests_present": s.proof_tests_present,
                    "proof_security_present": s.proof_security_present,
                    "proof_perf_present": s.proof_perf_present,
                    "proof_docs_present": s.proof_docs_present
                }))
            }));

            if summary.total_steps == 0 {
                next.push("Add steps to this task".to_string());
            } else {
                if summary.missing_criteria > 0 {
                    verify.push(format!(
                        "Missing criteria checkpoints: {}",
                        summary.missing_criteria
                    ));
                }
                if summary.missing_tests > 0 {
                    verify.push(format!(
                        "Missing tests checkpoints: {}",
                        summary.missing_tests
                    ));
                }
                if summary.missing_security > 0 {
                    verify.push(format!(
                        "Missing security checkpoints: {}",
                        summary.missing_security
                    ));
                }
                if summary.missing_perf > 0 {
                    verify.push(format!(
                        "Missing perf checkpoints: {}",
                        summary.missing_perf
                    ));
                }
                if summary.missing_docs > 0 {
                    verify.push(format!(
                        "Missing docs checkpoints: {}",
                        summary.missing_docs
                    ));
                }

                if let Some(first) = summary.first_open {
                    let require_security = first.require_security;
                    let require_perf = first.require_perf;
                    let require_docs = first.require_docs;
                    if !first.criteria_confirmed
                        || !first.tests_confirmed
                        || (require_security && !first.security_confirmed)
                        || (require_perf && !first.perf_confirmed)
                        || (require_docs && !first.docs_confirmed)
                    {
                        next.push(format!("Confirm checkpoints for {}", first.path));
                    } else {
                        next.push(format!("Close next step {}", first.path));
                    }
                } else if target.get("status").and_then(|v| v.as_str()) != Some("DONE") {
                    next.push("Finish task".to_string());
                }
            }
        }

        if let Ok(items) = store.task_open_blockers(workspace, target_id, 10) {
            blockers = items;
        }
    }

    let radar = json!({
        "now": now,
        "why": why,
        "verify": verify,
        "next": next,
        "blockers": blockers
    });

    Ok(RadarContext {
        target,
        reasoning_ref: reasoning_ref_json,
        radar,
        steps: steps_summary,
    })
}
