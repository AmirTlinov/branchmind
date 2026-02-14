#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_create(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let workspace = match require_workspace(args_obj) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "title",
            "prompt",
            "kind",
            "priority",
            "task",
            "anchor",
            "executor",
            "executor_profile",
            "executor_model",
            "policy",
            "expected_artifacts",
            "meta",
        ],
        "jobs.create",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let title = match require_string(args_obj, "title") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let prompt = match require_string(args_obj, "prompt") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let kind = match optional_string(args_obj, "kind") {
        Ok(v) => v.unwrap_or_else(|| "codex_cli".to_string()),
        Err(resp) => return resp,
    };
    let priority = match optional_string(args_obj, "priority") {
        Ok(v) => v.unwrap_or_else(|| "MEDIUM".to_string()),
        Err(resp) => return resp,
    };
    let task_id = match optional_string(args_obj, "task") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let anchor_id = match optional_string(args_obj, "anchor") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let executor = match optional_string(args_obj, "executor") {
        Ok(v) => v.unwrap_or_else(|| "auto".to_string()),
        Err(resp) => return resp,
    };
    let executor_profile = match optional_string(args_obj, "executor_profile") {
        Ok(v) => v.unwrap_or_else(|| "xhigh".to_string()),
        Err(resp) => return resp,
    };
    let executor_model = match optional_string(args_obj, "executor_model") {
        Ok(v) => v.unwrap_or_else(|| "gpt-5.3-codex".to_string()),
        Err(resp) => return resp,
    };
    let expected_artifacts = match optional_string_array(args_obj, "expected_artifacts") {
        Ok(v) => v.unwrap_or_default(),
        Err(resp) => return resp,
    };
    let policy = args_obj.get("policy").cloned().unwrap_or(Value::Null);

    let mut meta_obj = args_obj
        .get("meta")
        .cloned()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    meta_obj.insert("executor".to_string(), Value::String(executor.clone()));
    meta_obj.insert(
        "executor_profile".to_string(),
        Value::String(executor_profile.clone()),
    );
    meta_obj.insert(
        "executor_model".to_string(),
        Value::String(executor_model.clone()),
    );
    if !expected_artifacts.is_empty() {
        meta_obj.insert(
            "expected_artifacts".to_string(),
            Value::Array(
                expected_artifacts
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    if !policy.is_null() {
        meta_obj.insert("policy".to_string(), policy.clone());
    }

    if executor == "auto"
        && let Some(selection) = executor_routing::auto_route_executor(
            server,
            &workspace,
            &executor_profile,
            &expected_artifacts,
            &policy,
        )
    {
        meta_obj.insert("routing".to_string(), selection);
    }

    let meta_json = serde_json::to_string(&Value::Object(meta_obj)).ok();

    let created = match server.store.job_create(
        &workspace,
        bm_storage::JobCreateRequest {
            title,
            prompt,
            kind,
            priority,
            task_id,
            anchor_id,
            meta_json,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(created.job),
        "event": job_event_to_json(created.created_event)
    });

    // UX: try to auto-start the first-party runner when the job queue becomes non-empty.
    // Without this, users can end up with jobs stuck in QUEUED until they discover runner setup.
    //
    // We intentionally keep this best-effort and budget-safe:
    // - only attempt when runner is offline
    // - throttle is handled by maybe_autostart_runner()
    let now_ms = crate::support::now_ms_i64();
    let runner_status = server.store.runner_status_snapshot(&workspace, now_ms).ok();
    let runner_is_offline = runner_status
        .as_ref()
        .is_some_and(|s| s.status.as_str() == "offline");
    let runner_autostart_active =
        server.maybe_autostart_runner(&workspace, now_ms, 1, runner_is_offline);

    let mut result = result;
    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "runner_autostart".to_string(),
            json!({
                "active": runner_autostart_active
            }),
        );

        // If autostart is disabled or failed, provide a copy/paste bootstrap hint.
        if runner_is_offline && !runner_autostart_active {
            let storage_dir = server.store.storage_dir();
            let storage_dir =
                std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
            let mcp_bin =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("bm_mcp"));
            let runner_bin = mcp_bin
                .parent()
                .map(|dir| dir.join("bm_runner"))
                .filter(|p| p.exists())
                .unwrap_or_else(|| std::path::PathBuf::from("bm_runner"));

            let cmd = format!(
                "\"{}\" --storage-dir \"{}\" --workspace \"{}\" --mcp-bin \"{}\"",
                runner_bin.to_string_lossy(),
                storage_dir.to_string_lossy(),
                workspace.as_str(),
                mcp_bin.to_string_lossy()
            );
            obj.insert(
                "runner_bootstrap".to_string(),
                json!({
                    "cmd": cmd,
                    "runner_bin": runner_bin.to_string_lossy(),
                    "mcp_bin": mcp_bin.to_string_lossy(),
                    "storage_dir": storage_dir.to_string_lossy()
                }),
            );
        }
    }

    if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_create", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_create", result)
    }
}
