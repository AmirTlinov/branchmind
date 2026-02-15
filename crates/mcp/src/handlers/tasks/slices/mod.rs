#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

mod apply;
mod open;
mod propose_next;
mod validate;

fn action_call(cmd: &str, reason: &str, priority: &str, args: Value) -> Value {
    json!({
        "op": "call",
        "cmd": cmd,
        "reason": reason,
        "priority": priority,
        "budget_profile": "portal",
        "args": args
    })
}

fn check_unknown_args(
    args_obj: &serde_json::Map<String, Value>,
    allowed: &[&str],
    cmd: &str,
) -> Result<(), Value> {
    // Envelope/budget machinery may inject these keys for bounded responses; they are not
    // user-facing semantic args and should not trigger unknown-args failures.
    const IMPLICIT_ENVELOPE_KEYS: &[&str] = &["context_budget", "limit", "max_chars", "agent_id"];
    let mut unknown = args_obj
        .keys()
        .filter(|k| {
            !allowed.iter().any(|a| a == &k.as_str())
                && !IMPLICIT_ENVELOPE_KEYS.iter().any(|ik| ik == &k.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    unknown.dedup();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(ai_error_with(
        "INVALID_INPUT",
        &format!("unknown args for {cmd}: {}", unknown.join(", ")),
        Some("Remove unknown args and retry."),
        Vec::new(),
    ))
}

fn slice_actions_for_jobs(
    workspace: &str,
    plan_id: &str,
    slice_id: &str,
    objective: &str,
) -> Vec<Value> {
    vec![action_call(
        "jobs.macro.dispatch.scout",
        "Start slice scout (bounded context pack).",
        "high",
        json!({
            "workspace": workspace,
            "task": plan_id,
            "anchor": format!("a:{}", slice_id.to_ascii_lowercase()),
            "slice_id": slice_id,
            "objective": objective,
            "executor": "claude_code",
            "model": "haiku",
            "executor_profile": "deep",
            "quality_profile": "flagship"
        }),
    )]
}

fn slice_actions_after_apply(
    workspace: &str,
    plan_id: &str,
    slice_id: &str,
    objective: &str,
) -> Vec<Value> {
    let mut out = Vec::<Value>::new();
    out.push(action_call(
        "tasks.slice.open",
        "Open the slice plan + deterministic step tree.",
        "medium",
        json!({ "workspace": workspace, "slice_id": slice_id }),
    ));
    out.push(action_call(
        "tasks.slice.validate",
        "Validate slice plan structure (fail-closed).",
        "high",
        json!({ "workspace": workspace, "slice_id": slice_id, "policy": "fail_closed" }),
    ));
    out.extend(slice_actions_for_jobs(
        workspace, plan_id, slice_id, objective,
    ));
    out
}

fn slice_actions_for_open(
    workspace: &str,
    plan_id: &str,
    slice_id: &str,
    objective: &str,
) -> Vec<Value> {
    let mut out = Vec::<Value>::new();
    out.push(action_call(
        "tasks.slice.validate",
        "Validate slice plan structure (fail-closed).",
        "high",
        json!({ "workspace": workspace, "slice_id": slice_id, "policy": "fail_closed" }),
    ));
    out.extend(slice_actions_for_jobs(
        workspace, plan_id, slice_id, objective,
    ));
    out
}

#[cfg(test)]
mod tests {
    use bm_storage::SqliteStore;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bm_slice_plans_v1_smoke_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn slice_apply_validate_and_jobs_scout_dry_run_smoke() {
        let dir = temp_dir();
        let mut store = SqliteStore::open(&dir).expect("open store");
        let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
        store.workspace_init(&workspace).unwrap();

        let (plan_id, _rev, _event) = store
            .create(
                &workspace,
                bm_storage::TaskCreateRequest {
                    kind: crate::TaskKind::Plan,
                    title: "Slice Plans v1 Plan".to_string(),
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: json!({"kind":"plan","title":"Slice Plans v1 Plan"})
                        .to_string(),
                },
            )
            .expect("create plan");

        let runner_autostart_enabled =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let runner_autostart_state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
        let mut server = crate::McpServer::new(
            store,
            crate::McpServerConfig {
                toolset: crate::Toolset::Daily,
                response_verbosity: crate::ResponseVerbosity::Full,
                dx_mode: false,
                ux_proof_v2_enabled: true,
                jobs_unknown_args_fail_closed_enabled: true,
                jobs_strict_progress_schema_enabled: true,
                jobs_high_done_proof_gate_enabled: true,
                jobs_wait_stream_v2_enabled: true,
                jobs_mesh_v1_enabled: true,
                slice_plans_v1_enabled: true,
                jobs_slice_first_fail_closed_enabled: true,
                slice_budgets_enforced_enabled: true,
                default_workspace: Some("demo".to_string()),
                workspace_explicit: false,
                workspace_allowlist: None,
                workspace_lock: true,
                project_guard: None,
                project_guard_rebind_enabled: false,
                default_agent_id: None,
                runner_autostart_enabled,
                runner_autostart_dry_run: false,
                runner_autostart: runner_autostart_state,
            },
        );

        let spec = crate::support::propose_next_slice_spec(
            &plan_id,
            "Slice Plans v1 Plan",
            "Implement a small deterministic slice",
            &[],
        );
        let resp = server.tool_tasks_slices_apply(json!({
            "workspace": "demo",
            "plan": plan_id,
            "policy": "fail_closed",
            "slice_plan_spec": spec.to_json()
        }));
        assert_eq!(
            resp.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "expected success: {resp}"
        );
        let slice_id = resp
            .get("result")
            .and_then(|v| v.get("slice"))
            .and_then(|v| v.get("slice_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            slice_id.starts_with("SLC-"),
            "expected SLC-* slice_id: {slice_id}"
        );

        let validate = server.tool_tasks_slice_validate(json!({
            "workspace": "demo",
            "slice_id": slice_id,
            "policy": "fail_closed"
        }));
        assert_eq!(
            validate.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "expected validate success: {validate}"
        );
        assert_eq!(
            validate
                .get("result")
                .and_then(|v| v.get("status"))
                .and_then(|v| v.as_str()),
            Some("pass"),
            "expected pass: {validate}"
        );

        let open =
            server.tool_tasks_slice_open(json!({ "workspace": "demo", "slice_id": slice_id }));
        assert_eq!(
            open.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "expected open success: {open}"
        );

        let scout = server.tool_tasks_jobs_macro_dispatch_scout(json!({
            "workspace": "demo",
            "slice_id": slice_id,
            "dry_run": true
        }));
        assert_eq!(
            scout.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "expected scout dry_run success: {scout}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
