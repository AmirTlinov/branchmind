#![forbid(unsafe_code)]

use bm_storage::StoreError;
use serde_json::{Value, json};

pub(crate) fn format_store_error(err: StoreError) -> String {
    match err {
        StoreError::Io(e) => format!("IO: {e}"),
        StoreError::Sql(e) => format!("SQL: {e}"),
        StoreError::InvalidInput(msg) => format!("Invalid input: {msg}"),
        StoreError::ProjectGuardMismatch { expected, stored } => {
            format!("Project guard mismatch: expected={expected} stored={stored}")
        }
        StoreError::RevisionMismatch { expected, actual } => {
            format!("Revision mismatch: expected={expected} actual={actual}")
        }
        StoreError::UnknownId => "Unknown id".to_string(),
        StoreError::JobNotClaimable { job_id, status } => {
            format!("Job not claimable: job_id={job_id} status={status}")
        }
        StoreError::JobNotRunning { job_id, status } => {
            format!("Job not running: job_id={job_id} status={status}")
        }
        StoreError::JobNotCancelable { job_id, status } => {
            format!("Job not cancelable: job_id={job_id} status={status}")
        }
        StoreError::JobClaimMismatch {
            job_id,
            expected_runner_id,
            actual_runner_id,
            expected_revision,
            actual_revision,
        } => match expected_runner_id {
            Some(expected_runner_id) => format!(
                "Job claim mismatch: job_id={job_id} expected_runner_id={expected_runner_id} actual_runner_id={actual_runner_id} expected_revision={expected_revision} actual_revision={actual_revision}"
            ),
            None => format!(
                "Job claim mismatch: job_id={job_id} expected_runner_id=<none> actual_runner_id={actual_runner_id} expected_revision={expected_revision} actual_revision={actual_revision}"
            ),
        },
        StoreError::JobNotMessageable { job_id, status } => {
            format!("Job not messageable: job_id={job_id} status={status}")
        }
        StoreError::JobAlreadyTerminal { job_id, status } => {
            format!("Job already terminal: job_id={job_id} status={status}")
        }
        StoreError::JobNotRequeueable { job_id, status } => {
            format!("Job not requeueable: job_id={job_id} status={status}")
        }
        StoreError::UnknownBranch => "Unknown branch".to_string(),
        StoreError::UnknownConflict => "Unknown conflict".to_string(),
        StoreError::ConflictAlreadyResolved => "Conflict already resolved".to_string(),
        StoreError::MergeNotSupported => "Merge not supported".to_string(),
        StoreError::BranchAlreadyExists => "Branch already exists".to_string(),
        StoreError::BranchCycle => "Branch base cycle".to_string(),
        StoreError::BranchDepthExceeded => "Branch base depth exceeded".to_string(),
        StoreError::StepNotFound => "Step not found".to_string(),
        StoreError::CheckpointsNotConfirmed {
            criteria,
            tests,
            security,
            perf,
            docs,
        } => format!(
            "Checkpoints not confirmed: criteria={criteria} tests={tests} security={security} perf={perf} docs={docs}"
        ),
        StoreError::ProofMissing {
            tests,
            security,
            perf,
            docs,
        } => format!("Proof missing: tests={tests} security={security} perf={perf} docs={docs}"),
        StoreError::StepLeaseHeld {
            step_id,
            holder_agent_id,
            now_seq,
            expires_seq,
        } => format!(
            "Step lease held: step_id={step_id} holder={holder_agent_id} now_seq={now_seq} expires_seq={expires_seq}"
        ),
        StoreError::StepLeaseNotHeld {
            step_id,
            holder_agent_id,
        } => match holder_agent_id {
            None => format!("Step lease not held: step_id={step_id} (no active lease)"),
            Some(holder) => format!("Step lease not held: step_id={step_id} holder={holder}"),
        },
    }
}

pub(crate) fn warning(code: &str, message: &str, recovery: &str) -> Value {
    json!({
        "code": code,
        "message": message,
        "recovery": recovery
    })
}

pub(crate) fn ai_ok_with_warnings(
    intent: &str,
    result: Value,
    warnings: Vec<Value>,
    refs: Vec<Value>,
) -> Value {
    json!({
        "success": true,
        "intent": intent,
        "result": result,
        "warnings": warnings,
        "refs": refs,
        "error": null
    })
}

pub(crate) fn ai_ok(intent: &str, result: Value) -> Value {
    ai_ok_with_warnings(intent, result, Vec::new(), Vec::new())
}

pub(crate) fn ai_error_with(
    code: &str,
    message: &str,
    recovery: Option<&str>,
    refs: Vec<Value>,
) -> Value {
    let mut error_obj = serde_json::Map::new();
    error_obj.insert("code".to_string(), Value::String(code.to_string()));
    error_obj.insert(
        "message".to_string(),
        Value::String(message.trim().to_string()),
    );
    if let Some(recovery) = recovery {
        error_obj.insert(
            "recovery".to_string(),
            Value::String(recovery.trim().to_string()),
        );
    }

    json!({
        "success": false,
        "intent": "error",
        "result": {},
        "warnings": [],
        "refs": refs,
        "error": Value::Object(error_obj)
    })
}
