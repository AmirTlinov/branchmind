#![forbid(unsafe_code)]

use super::*;
use serde_json::{Value, json};

pub(super) fn tool_tasks_runner_heartbeat(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "runner_id",
            "status",
            "active_job_id",
            "lease_ttl_ms",
            "executors",
            "profiles",
            "supports_artifacts",
            "max_parallel",
            "sandbox_policy",
            "meta",
        ],
        "jobs.runner.heartbeat",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let workspace = match require_workspace(args_obj) {
        Ok(w) => w,
        Err(resp) => return resp,
    };

    let runner_id = match require_string(args_obj, "runner_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let status = match require_string(args_obj, "status") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let active_job_id = match optional_string(args_obj, "active_job_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let lease_ttl_ms = match optional_usize(args_obj, "lease_ttl_ms") {
        Ok(v) => v.unwrap_or(20_000).clamp(1_000, 300_000) as u64,
        Err(resp) => return resp,
    };
    let mut meta_obj = args_obj
        .get("meta")
        .cloned()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if let Some(executors) = args_obj.get("executors").and_then(|v| v.as_array()) {
        meta_obj.insert("executors".to_string(), Value::Array(executors.clone()));
    }
    if let Some(profiles) = args_obj.get("profiles").and_then(|v| v.as_array()) {
        meta_obj.insert("profiles".to_string(), Value::Array(profiles.clone()));
    }
    if let Some(artifacts) = args_obj
        .get("supports_artifacts")
        .and_then(|v| v.as_array())
    {
        meta_obj.insert(
            "supports_artifacts".to_string(),
            Value::Array(artifacts.clone()),
        );
    }
    if let Some(max_parallel) = args_obj.get("max_parallel") {
        meta_obj.insert("max_parallel".to_string(), max_parallel.clone());
    }
    if let Some(sandbox_policy) = args_obj.get("sandbox_policy") {
        meta_obj.insert("sandbox_policy".to_string(), sandbox_policy.clone());
    }
    let meta_json = serde_json::to_string(&Value::Object(meta_obj)).ok();

    let lease = match server.store.runner_lease_upsert(
        &workspace,
        bm_storage::RunnerLeaseUpsertRequest {
            runner_id,
            status,
            active_job_id,
            lease_ttl_ms,
            meta_json,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "lease": runner_lease_to_json(lease)
    });
    if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_runner_heartbeat", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_runner_heartbeat", result)
    }
}
