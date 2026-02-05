#![forbid(unsafe_code)]

use super::ViewerConfig;
use super::snapshot::{SnapshotError, store_err};
use crate::{now_ms_i64, now_rfc3339};
use bm_core::ids::WorkspaceId;
use bm_storage::{AnchorsListRequest, KnowledgeKeysListAnyRequest, SqliteStore};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

const MAX_ANCHORS: usize = 200;
const MAX_KEYS: usize = 800;
const RUNNER_STARTING_WINDOW_MS: i64 = 15_000;

#[derive(Clone, Copy, Debug, Default)]
struct KeyCounts {
    total: i64,
}

pub(crate) fn build_knowledge_snapshot(
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
        return Ok(empty_knowledge_snapshot(
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

    let anchors_total = store
        .count_anchors(&workspace)
        .map_err(store_err("STORE_ERROR"))?;
    let keys_total = store
        .count_knowledge_keys(&workspace)
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
    if runner_snapshot.status == "offline"
        && runner_autostart_enabled
        && (jobs_counts.queued > 0 || runner_autostart_active || runner_autostart_attempt_recent)
    {
        runner_status_ui = "starting".to_string();
    }

    let anchors = store
        .anchors_list(
            &workspace,
            AnchorsListRequest {
                text: None,
                kind: None,
                status: None,
                limit: MAX_ANCHORS,
            },
        )
        .map_err(store_err("STORE_ERROR"))?
        .anchors;

    let anchor_ids = anchors.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    let counts_by_anchor_raw = store
        .count_knowledge_keys_for_anchors(&workspace, &anchor_ids)
        .map_err(store_err("STORE_ERROR"))?;

    let mut counts_by_anchor: BTreeMap<String, KeyCounts> = BTreeMap::new();
    for (anchor_id, count) in counts_by_anchor_raw {
        counts_by_anchor.insert(anchor_id, KeyCounts { total: count });
    }

    let keys = store
        .knowledge_keys_list_any(
            &workspace,
            KnowledgeKeysListAnyRequest {
                anchor_ids: Vec::new(),
                limit: MAX_KEYS,
            },
        )
        .map_err(store_err("STORE_ERROR"))?
        .items;

    let focus_id = store.focus_get(&workspace).ok().flatten();
    let focus = resolve_work_focus(store, &workspace, focus_id.as_deref());

    let primary_anchor_id = anchors.first().map(|anchor| anchor.id.clone());

    let plans_json = anchors
        .iter()
        .map(|anchor| {
            let counts = counts_by_anchor
                .get(&anchor.id)
                .copied()
                .unwrap_or_default();
            json!({
                "id": anchor.id.clone(),
                "title": anchor.title.clone(),
                "description": anchor.description.clone(),
                "context": Value::Null,
                "status": anchor.status.clone(),
                "priority": anchor.kind.clone(),
                "updated_at_ms": anchor.updated_at_ms,
                "task_counts": {
                    "total": counts.total,
                    "active": counts.total,
                    "backlog": 0,
                    "parked": 0,
                    "done": 0
                },
                "kind": anchor.kind.clone(),
                "refs": anchor.refs.clone(),
                "aliases": anchor.aliases.clone(),
                "parent_id": anchor.parent_id.clone(),
                "depends_on": anchor.depends_on.clone()
            })
        })
        .collect::<Vec<_>>();

    let tasks_json = keys
        .iter()
        .map(|row| {
            json!({
                "id": knowledge_key_task_id(&row.anchor_id, &row.key),
                "plan_id": row.anchor_id.clone(),
                "title": row.key.clone(),
                "description": Value::Null,
                "context": row.card_id.clone(),
                "status": "TODO",
                "priority": "MEDIUM",
                "blocked": false,
                "updated_at_ms": row.updated_at_ms,
                "parked_until_ts_ms": Value::Null,
                "card_id": row.card_id.clone(),
                "key": row.key.clone(),
                "anchor_id": row.anchor_id.clone()
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "lens": "knowledge",
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
        "primary_plan_id": primary_anchor_id,
        "plans_total": anchors_total,
        "tasks_total": keys_total,
        "plans": plans_json,
        "plan_checklist": Value::Null,
        "plan_checklists": json!({}),
        "tasks": tasks_json,
        "truncated": {
            "plans": anchors_total as usize > MAX_ANCHORS,
            "tasks": keys_total as usize > MAX_KEYS
        }
    }))
}

pub(crate) fn knowledge_key_task_id(anchor_id: &str, key: &str) -> String {
    let anchor_id = anchor_id.trim();
    let key = key.trim();
    if anchor_id.is_empty() || key.is_empty() {
        return "KN:invalid".to_string();
    }
    format!("KN:{anchor_id}:{key}")
}

fn empty_knowledge_snapshot(
    workspace: &str,
    generated_at: Value,
    generated_at_ms: i64,
    expected_guard: Option<&str>,
) -> Value {
    json!({
        "lens": "knowledge",
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
            "autostart": { "enabled": false, "dry_run": false, "active": false, "last_attempt_ms": Value::Null, "last_attempt_ok": Value::Null }
        },
        "focus": { "kind": "none", "id": Value::Null, "title": Value::Null, "plan_id": Value::Null },
        "primary_plan_id": Value::Null,
        "plans_total": 0,
        "tasks_total": 0,
        "plans": [],
        "plan_checklist": Value::Null,
        "plan_checklists": json!({}),
        "tasks": [],
        "truncated": { "plans": false, "tasks": false }
    })
}

fn resolve_work_focus(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    focus_id: Option<&str>,
) -> Option<Value> {
    let focus_id = focus_id?.trim();
    if focus_id.is_empty() {
        return None;
    }
    if focus_id.starts_with("PLAN-") {
        let plan = store.get_plan(workspace, focus_id).ok().flatten()?;
        return Some(json!({
            "kind": "plan",
            "id": plan.id.clone(),
            "title": plan.title.clone(),
            "plan_id": plan.id.clone()
        }));
    }
    if focus_id.starts_with("TASK-") {
        let task = store.get_task(workspace, focus_id).ok().flatten()?;
        return Some(json!({
            "kind": "task",
            "id": task.id.clone(),
            "title": task.title.clone(),
            "plan_id": task.parent_plan_id.clone()
        }));
    }
    None
}
