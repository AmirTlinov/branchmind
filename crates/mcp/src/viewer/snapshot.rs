#![forbid(unsafe_code)]

use crate::viewer::ViewerConfig;
use crate::{now_ms_i64, now_rfc3339};
use bm_core::ids::WorkspaceId;
use bm_storage::{PlanRow, SqliteStore, StoreError, TaskRow};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

const MAX_PLANS: usize = 200;
const MAX_TASKS: usize = 800;
const RUNNER_STARTING_WINDOW_MS: i64 = 15_000;

#[derive(Debug)]
pub(crate) struct SnapshotError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) recovery: Option<String>,
    pub(crate) status: u16,
}

impl SnapshotError {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "recovery": self.recovery
            }
        })
    }

    pub(crate) fn status_line(&self) -> &'static str {
        match self.status {
            400 => "400 Bad Request",
            404 => "404 Not Found",
            409 => "409 Conflict",
            503 => "503 Service Unavailable",
            500 => "500 Internal Server Error",
            _ => "500 Internal Server Error",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TaskCounts {
    total: i64,
    active: i64,
    backlog: i64,
    parked: i64,
    done: i64,
}

pub(crate) fn build_snapshot(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
) -> Result<Value, SnapshotError> {
    let generated_at_ms = now_ms_i64();
    let generated_at = now_rfc3339();
    let expected_guard = config.project_guard.as_deref();

    let workspace_raw = workspace_override
        .or(config.workspace.as_deref())
        .ok_or_else(|| SnapshotError {
            code: "WORKSPACE_REQUIRED",
            message: "Viewer requires a default workspace.".to_string(),
            recovery: Some(
                "Start the server with --workspace (or BRANCHMIND_WORKSPACE).".to_string(),
            ),
            status: 400,
        })?;
    let workspace = WorkspaceId::try_new(workspace_raw.to_string()).map_err(|_| SnapshotError {
        code: "INVALID_WORKSPACE",
        message: "workspace: expected WorkspaceId.".to_string(),
        recovery: Some("Use an alphanumeric workspace id (e.g. my-workspace).".to_string()),
        status: 400,
    })?;

    let exists = store
        .workspace_exists(&workspace)
        .map_err(store_err("STORE_ERROR"))?;
    if !exists {
        return Ok(empty_snapshot(
            workspace.as_str(),
            generated_at,
            generated_at_ms,
            expected_guard,
        ));
    }

    let mut stored_guard: Option<String> = None;
    let mut guard_status = "not_applicable";
    if let Some(expected_guard) = expected_guard {
        stored_guard = store
            .workspace_project_guard_get(&workspace)
            .map_err(store_err("STORE_ERROR"))?;
        match stored_guard.as_deref() {
            Some(stored) if stored == expected_guard => {
                guard_status = "ok";
            }
            Some(stored) => {
                return Err(SnapshotError {
                    code: "PROJECT_GUARD_MISMATCH",
                    message: "Workspace belongs to a different project guard.".to_string(),
                    recovery: Some(format!(
                        "Expected project_guard={expected_guard}, but workspace is guarded as {stored}."
                    )),
                    status: 409,
                });
            }
            None => {
                // Viewer-only UX: older stores might not have initialized workspace guards yet.
                // We still allow read-only browsing, but surface the status for caution.
                guard_status = "uninitialized";
            }
        }
    }

    let plan_total = store
        .count_plans(&workspace)
        .map_err(store_err("STORE_ERROR"))?;
    let task_total = store
        .count_tasks(&workspace)
        .map_err(store_err("STORE_ERROR"))?;

    let jobs_counts = store
        .jobs_status_counts(&workspace)
        .map_err(store_err("STORE_ERROR"))?;
    let runner_snapshot = store
        .runner_status_snapshot(&workspace, generated_at_ms)
        .map_err(store_err("STORE_ERROR"))?;

    let runner_autostart_enabled = config
        .runner_autostart_enabled
        .as_ref()
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false);
    let mut runner_autostart_last_attempt_ms: Option<i64> = None;
    let mut runner_autostart_last_attempt_ok: Option<bool> = None;
    let mut runner_autostart_active = false;
    if let Some(state) = config.runner_autostart.as_ref()
        && let Ok(state) = state.lock()
        && let Some(entry) = state.entries.get(workspace.as_str())
    {
        runner_autostart_last_attempt_ms = Some(entry.last_attempt_ms);
        runner_autostart_last_attempt_ok = Some(entry.last_attempt_ok);
        runner_autostart_active = entry.last_attempt_ok && entry.child.is_some();
    }

    let runner_autostart_attempt_recent = runner_autostart_last_attempt_ms
        .is_some_and(|ms| generated_at_ms.saturating_sub(ms) < RUNNER_STARTING_WINDOW_MS);

    let mut runner_status_ui = runner_snapshot.status.clone();
    if runner_snapshot.status == "offline" && runner_autostart_enabled {
        // "Soft" status: if work is queued (or we attempted autostart very recently),
        // show a transient "starting" instead of a harsh "offline" for a few seconds.
        if jobs_counts.queued > 0 || runner_autostart_active || runner_autostart_attempt_recent {
            runner_status_ui = "starting".to_string();
        }
    }

    let plans = store
        .list_plans(&workspace, MAX_PLANS, 0)
        .map_err(store_err("STORE_ERROR"))?;
    let tasks = store
        .list_tasks(&workspace, MAX_TASKS, 0)
        .map_err(store_err("STORE_ERROR"))?;

    let mut counts_by_plan: BTreeMap<String, TaskCounts> = BTreeMap::new();
    for task in tasks.iter() {
        let entry = counts_by_plan
            .entry(task.parent_plan_id.clone())
            .or_default();
        entry.total += 1;
        match task.status.as_str() {
            "DONE" => entry.done += 1,
            "PARKED" => entry.parked += 1,
            "TODO" => entry.backlog += 1,
            _ => entry.active += 1,
        }
    }

    let focus_id = store.focus_get(&workspace).ok().flatten();
    let focus = resolve_focus(store, &workspace, &plans, &tasks, focus_id.as_deref());

    let primary_plan_id = pick_primary_plan_id(&plans, focus.as_ref());
    let plan_checklist = match primary_plan_id.as_deref() {
        Some(plan_id) => store
            .plan_checklist_get(&workspace, plan_id)
            .ok()
            .map(|checklist| {
                json!({
                    "plan_id": plan_id,
                    "current": checklist.current,
                    "steps": checklist.steps
                })
            })
            .unwrap_or(Value::Null),
        None => Value::Null,
    };
    let mut plan_checklists = serde_json::Map::new();
    for plan in plans.iter() {
        if let Ok(checklist) = store.plan_checklist_get(&workspace, &plan.id) {
            plan_checklists.insert(
                plan.id.clone(),
                json!({
                    "plan_id": plan.id,
                    "current": checklist.current,
                    "steps": checklist.steps
                }),
            );
        }
    }

    let plans_json = plans
        .iter()
        .map(|plan| {
            let counts = counts_by_plan.get(&plan.id).copied().unwrap_or_default();
            json!({
                "id": plan.id.clone(),
                "title": plan.title.clone(),
                "description": plan.description.clone(),
                "context": plan.context.clone(),
                "status": plan.status.clone(),
                "priority": plan.priority.clone(),
                "updated_at_ms": plan.updated_at_ms,
                "task_counts": {
                    "total": counts.total,
                    "active": counts.active,
                    "backlog": counts.backlog,
                    "parked": counts.parked,
                    "done": counts.done
                }
            })
        })
        .collect::<Vec<_>>();

    let tasks_json = tasks
        .iter()
        .map(|task| {
            json!({
                "id": task.id.clone(),
                "plan_id": task.parent_plan_id.clone(),
                "title": task.title.clone(),
                "description": task.description.clone(),
                "context": task.context.clone(),
                "status": task.status.clone(),
                "priority": task.priority.clone(),
                "blocked": task.blocked,
                "updated_at_ms": task.updated_at_ms,
                "parked_until_ts_ms": task.parked_until_ts_ms
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "workspace": workspace.as_str(),
        "workspace_exists": true,
        "project_guard": {
            "expected": expected_guard,
            "stored": stored_guard.as_deref(),
            "status": guard_status
        },
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "runner": {
            "status": runner_status_ui,
            "base_status": runner_snapshot.status,
            "runner_id": runner_snapshot.runner_id,
            "active_job_id": runner_snapshot.active_job_id,
            "lease_expires_at_ms": runner_snapshot.lease_expires_at_ms,
            "live_count": runner_snapshot.live_count,
            "idle_count": runner_snapshot.idle_count,
            "offline_count": runner_snapshot.offline_count,
            "jobs": {
                "running": jobs_counts.running,
                "queued": jobs_counts.queued
            },
            "autostart": {
                "enabled": runner_autostart_enabled,
                "dry_run": config.runner_autostart_dry_run,
                "active": runner_autostart_active,
                "last_attempt_ms": runner_autostart_last_attempt_ms,
                "last_attempt_ok": runner_autostart_last_attempt_ok
            }
        },
        "focus": focus.unwrap_or_else(|| {
            json!({
                "kind": "none",
                "id": Value::Null,
                "title": Value::Null,
                "plan_id": Value::Null
            })
        }),
        "primary_plan_id": primary_plan_id,
        "plans": plans_json,
        "plan_checklist": plan_checklist,
        "plan_checklists": Value::Object(plan_checklists),
        "tasks": tasks_json,
        "truncated": {
            "plans": plan_total as usize > MAX_PLANS,
            "tasks": task_total as usize > MAX_TASKS
        }
    }))
}

fn empty_snapshot(
    workspace: &str,
    generated_at: Value,
    generated_at_ms: i64,
    expected_guard: Option<&str>,
) -> Value {
    json!({
        "workspace": workspace,
        "workspace_exists": false,
        "project_guard": {
            "expected": expected_guard,
            "stored": Value::Null,
            "status": if expected_guard.is_some() { "unknown" } else { "not_applicable" }
        },
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "runner": {
            "status": "offline",
            "base_status": "offline",
            "runner_id": Value::Null,
            "active_job_id": Value::Null,
            "lease_expires_at_ms": Value::Null,
            "live_count": 0,
            "idle_count": 0,
            "offline_count": 0,
            "jobs": { "running": 0, "queued": 0 },
            "autostart": {
                "enabled": false,
                "dry_run": false,
                "active": false,
                "last_attempt_ms": Value::Null,
                "last_attempt_ok": Value::Null
            }
        },
        "focus": {
            "kind": "none",
            "id": Value::Null,
            "title": Value::Null,
            "plan_id": Value::Null
        },
        "primary_plan_id": Value::Null,
        "plans": [],
        "plan_checklist": Value::Null,
        "plan_checklists": {},
        "tasks": [],
        "truncated": { "plans": false, "tasks": false }
    })
}

fn pick_primary_plan_id(plans: &[PlanRow], focus: Option<&Value>) -> Option<String> {
    if let Some(focus) = focus
        && let Some(kind) = focus.get("kind").and_then(|v| v.as_str())
        && let Some(id) = focus.get("id").and_then(|v| v.as_str())
    {
        if kind == "plan" {
            return Some(id.to_string());
        }
        if kind == "task"
            && let Some(plan_id) = focus.get("plan_id").and_then(|v| v.as_str())
        {
            return Some(plan_id.to_string());
        }
    }

    let mut active = plans.iter().filter(|p| p.status != "DONE");
    if let Some(plan) = active.next() {
        return Some(plan.id.clone());
    }
    plans.first().map(|plan| plan.id.clone())
}

fn resolve_focus(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    plans: &[PlanRow],
    tasks: &[TaskRow],
    focus_id: Option<&str>,
) -> Option<Value> {
    let focus_id = focus_id?;
    if focus_id.starts_with("PLAN-") {
        let plan = plans
            .iter()
            .find(|plan| plan.id == focus_id)
            .cloned()
            .or_else(|| store.get_plan(workspace, focus_id).ok().flatten())?;
        return Some(json!({
            "kind": "plan",
            "id": plan.id.clone(),
            "title": plan.title.clone(),
            "plan_id": plan.id.clone()
        }));
    }
    if focus_id.starts_with("TASK-") {
        let task = tasks
            .iter()
            .find(|task| task.id == focus_id)
            .cloned()
            .or_else(|| store.get_task(workspace, focus_id).ok().flatten())?;
        return Some(json!({
            "kind": "task",
            "id": task.id.clone(),
            "title": task.title.clone(),
            "plan_id": task.parent_plan_id.clone()
        }));
    }
    None
}

pub(crate) fn store_err(code: &'static str) -> impl Fn(StoreError) -> SnapshotError {
    move |err| {
        let message = err.to_string();
        // A common transient case when inspecting another project is: the DB file exists,
        // but schema has not been created yet (race with first open/migrations).
        if message.contains("no such table") {
            return SnapshotError {
                code: "PROJECT_UNAVAILABLE",
                message: "Project store is initializing.".to_string(),
                recovery: Some("Retry shortly.".to_string()),
                status: 503,
            };
        }
        SnapshotError {
            code,
            message: format!("Store error: {err}"),
            recovery: Some("Retry after fixing the store error.".to_string()),
            status: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::model::TaskKind;
    use bm_storage::{JobCreateRequest, TaskCreateRequest};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bm_viewer_test_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn snapshot_empty_workspace_exists_false() {
        let dir = temp_dir();
        let mut store = SqliteStore::open(&dir).unwrap();
        let config = ViewerConfig {
            storage_dir: dir.clone(),
            workspace: Some("demo".to_string()),
            project_guard: None,
            port: 0,
            runner_autostart_enabled: None,
            runner_autostart_dry_run: false,
            runner_autostart: None,
        };
        let snapshot = build_snapshot(&mut store, &config, None).unwrap();
        assert_eq!(
            snapshot.get("workspace_exists").and_then(|v| v.as_bool()),
            Some(false)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_includes_plan_and_task() {
        let dir = temp_dir();
        let mut store = SqliteStore::open(&dir).unwrap();
        let workspace = WorkspaceId::try_new("demo".to_string()).unwrap();
        store.workspace_init(&workspace).unwrap();

        let (plan_id, _, _) = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Plan,
                    title: "Goal Alpha".to_string(),
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .unwrap();

        let _ = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Task,
                    title: "Task One".to_string(),
                    parent_plan_id: Some(plan_id.clone()),
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "task_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .unwrap();

        let config = ViewerConfig {
            storage_dir: dir.clone(),
            workspace: Some("demo".to_string()),
            project_guard: None,
            port: 0,
            runner_autostart_enabled: None,
            runner_autostart_dry_run: false,
            runner_autostart: None,
        };
        let snapshot = build_snapshot(&mut store, &config, None).unwrap();
        let plans = snapshot.get("plans").and_then(|v| v.as_array()).unwrap();
        let tasks = snapshot.get("tasks").and_then(|v| v.as_array()).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(tasks.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_does_not_rebind_project_guard() {
        let dir = temp_dir();
        let mut store = SqliteStore::open(&dir).unwrap();
        let workspace = WorkspaceId::try_new("demo".to_string()).unwrap();
        store.workspace_init(&workspace).unwrap();
        store
            .workspace_project_guard_ensure(&workspace, "repo:aaaaaaaaaaaaaaaa")
            .unwrap();

        let _ = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Plan,
                    title: "Goal Alpha".to_string(),
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .unwrap();

        let config = ViewerConfig {
            storage_dir: dir.clone(),
            workspace: Some("demo".to_string()),
            project_guard: Some("repo:bbbbbbbbbbbbbbbb".to_string()),
            port: 0,
            runner_autostart_enabled: None,
            runner_autostart_dry_run: false,
            runner_autostart: None,
        };
        let err = build_snapshot(&mut store, &config, None).unwrap_err();
        assert_eq!(err.code, "PROJECT_GUARD_MISMATCH");

        let stored = store
            .workspace_project_guard_get(&workspace)
            .unwrap()
            .unwrap();
        assert_eq!(stored, "repo:aaaaaaaaaaaaaaaa");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_runner_starting_when_recent_autostart_and_jobs_queued() {
        let dir = temp_dir();
        let mut store = SqliteStore::open(&dir).unwrap();
        let workspace = WorkspaceId::try_new("demo".to_string()).unwrap();
        store.workspace_init(&workspace).unwrap();

        let _ = store
            .job_create(
                &workspace,
                JobCreateRequest {
                    title: "Background job".to_string(),
                    prompt: "Do something".to_string(),
                    kind: "test".to_string(),
                    priority: "medium".to_string(),
                    task_id: None,
                    anchor_id: None,
                    meta_json: None,
                },
            )
            .unwrap();

        let runner_autostart_enabled = Arc::new(AtomicBool::new(true));
        let runner_autostart_state = Arc::new(Mutex::new(crate::RunnerAutostartState::default()));
        {
            let mut state = runner_autostart_state.lock().unwrap();
            state.entries.insert(
                "demo".to_string(),
                crate::RunnerAutostartEntry {
                    last_attempt_ms: now_ms_i64(),
                    last_attempt_ok: true,
                    child: None,
                },
            );
        }

        let config = ViewerConfig {
            storage_dir: dir.clone(),
            workspace: Some("demo".to_string()),
            project_guard: None,
            port: 0,
            runner_autostart_enabled: Some(runner_autostart_enabled),
            runner_autostart_dry_run: false,
            runner_autostart: Some(runner_autostart_state),
        };
        let snapshot = build_snapshot(&mut store, &config, None).unwrap();
        let runner_status = snapshot
            .get("runner")
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str());
        assert_eq!(runner_status, Some("starting"));

        let _ = fs::remove_dir_all(&dir);
    }
}
