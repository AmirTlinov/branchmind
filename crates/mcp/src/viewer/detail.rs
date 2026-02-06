#![forbid(unsafe_code)]

use super::ViewerConfig;
use super::snapshot::SnapshotError;
use super::snapshot::store_err;
use crate::{now_ms_i64, now_rfc3339};
use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use bm_storage::SqliteStore;
use serde_json::{Value, json};

const MAX_STEP_ITEMS: usize = 400;
const MAX_DOC_TAIL_ENTRIES: usize = 64;
const MAX_ENTRY_CONTENT_CHARS: usize = 20_000;

pub(super) struct ResolvedWorkspace {
    pub(super) workspace: WorkspaceId,
    pub(super) stored_guard: Option<String>,
    pub(super) guard_status: &'static str,
}

pub(crate) fn build_task_detail(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    task_id: &str,
    trace_cursor: Option<i64>,
    notes_cursor: Option<i64>,
) -> Result<Value, SnapshotError> {
    let task_id = task_id.trim();
    if !is_valid_task_id(task_id) {
        return Err(SnapshotError {
            code: "INVALID_TASK_ID",
            message: "task_id: expected TASK-###".to_string(),
            recovery: Some("Use a TASK id like TASK-278.".to_string()),
            status: 400,
        });
    }

    let resolved = resolve_workspace(store, config, workspace_override)?;
    let workspace = resolved.workspace.clone();

    let Some(task) = store
        .get_task(&workspace, task_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_TASK",
            message: "Task not found.".to_string(),
            recovery: Some("Pick an existing TASK id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let steps_limit = MAX_STEP_ITEMS.saturating_add(1);
    let mut steps = store
        .list_task_steps(&workspace, &task.id, None, steps_limit)
        .map_err(store_err("STORE_ERROR"))?;
    let steps_truncated = steps.len() > MAX_STEP_ITEMS;
    if steps_truncated {
        steps.truncate(MAX_STEP_ITEMS);
    }

    let reasoning = resolve_reasoning_ref(store, &workspace, TaskKind::Task, &task.id)?;
    let trace_tail = doc_tail_json(
        store,
        &workspace,
        &reasoning.branch,
        &reasoning.trace_doc,
        trace_cursor,
    )?;
    let notes_tail = doc_tail_json(
        store,
        &workspace,
        &reasoning.branch,
        &reasoning.notes_doc,
        notes_cursor,
    )?;

    Ok(json!({
        "workspace": workspace.as_str(),
        "project_guard": {
            "expected": config.project_guard.as_deref(),
            "stored": resolved.stored_guard.as_deref(),
            "status": resolved.guard_status
        },
        "generated_at": now_rfc3339(),
        "generated_at_ms": now_ms_i64(),
        "task": {
            "id": task.id,
            "plan_id": task.parent_plan_id,
            "title": task.title,
            "description": task.description,
            "context": task.context,
            "status": task.status,
            "priority": task.priority,
            "blocked": task.blocked,
            "updated_at_ms": task.updated_at_ms,
            "parked_until_ts_ms": task.parked_until_ts_ms
        },
        "steps": {
            "items": steps.into_iter().map(|step| json!({
                "path": step.path,
                "title": step.title,
                "completed": step.completed,
                "created_at_ms": step.created_at_ms,
                "updated_at_ms": step.updated_at_ms,
                "completed_at_ms": step.completed_at_ms,
                "criteria_confirmed": step.criteria_confirmed,
                "tests_confirmed": step.tests_confirmed,
                "security_confirmed": step.security_confirmed,
                "perf_confirmed": step.perf_confirmed,
                "docs_confirmed": step.docs_confirmed,
                "blocked": step.blocked,
                "block_reason": step.block_reason
            })).collect::<Vec<_>>(),
            "truncated": steps_truncated
        },
        "trace_tail": trace_tail,
        "notes_tail": notes_tail
    }))
}

pub(crate) fn build_plan_detail(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    plan_id: &str,
    trace_cursor: Option<i64>,
    notes_cursor: Option<i64>,
) -> Result<Value, SnapshotError> {
    let plan_id = plan_id.trim();
    if !is_valid_plan_id(plan_id) {
        return Err(SnapshotError {
            code: "INVALID_PLAN_ID",
            message: "plan_id: expected PLAN-###".to_string(),
            recovery: Some("Use a PLAN id like PLAN-052.".to_string()),
            status: 400,
        });
    }

    let resolved = resolve_workspace(store, config, workspace_override)?;
    let workspace = resolved.workspace.clone();

    let Some(plan) = store
        .get_plan(&workspace, plan_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_PLAN",
            message: "Plan not found.".to_string(),
            recovery: Some("Pick an existing PLAN id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let reasoning = resolve_reasoning_ref(store, &workspace, TaskKind::Plan, &plan.id)?;
    let trace_tail = doc_tail_json(
        store,
        &workspace,
        &reasoning.branch,
        &reasoning.trace_doc,
        trace_cursor,
    )?;
    let notes_tail = doc_tail_json(
        store,
        &workspace,
        &reasoning.branch,
        &reasoning.notes_doc,
        notes_cursor,
    )?;

    Ok(json!({
        "workspace": workspace.as_str(),
        "project_guard": {
            "expected": config.project_guard.as_deref(),
            "stored": resolved.stored_guard.as_deref(),
            "status": resolved.guard_status
        },
        "generated_at": now_rfc3339(),
        "generated_at_ms": now_ms_i64(),
        "plan": {
            "id": plan.id,
            "title": plan.title,
            "description": plan.description,
            "context": plan.context,
            "status": plan.status,
            "priority": plan.priority,
            "updated_at_ms": plan.updated_at_ms
        },
        "trace_tail": trace_tail,
        "notes_tail": notes_tail
    }))
}

pub(super) fn resolve_workspace(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
) -> Result<ResolvedWorkspace, SnapshotError> {
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
        return Err(SnapshotError {
            code: "WORKSPACE_NOT_FOUND",
            message: "Workspace does not exist yet.".to_string(),
            recovery: Some("Open the workspace via MCP once to initialize it.".to_string()),
            status: 404,
        });
    }

    let mut stored_guard: Option<String> = None;
    let mut guard_status = "not_applicable";
    if let Some(expected_guard) = config.project_guard.as_deref() {
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
                // Allow read-only browsing, but surface the status for caution.
                guard_status = "uninitialized";
            }
        }
    }

    Ok(ResolvedWorkspace {
        workspace,
        stored_guard,
        guard_status,
    })
}

fn resolve_reasoning_ref(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    kind: TaskKind,
    id: &str,
) -> Result<ReasoningRef, SnapshotError> {
    let existing = store
        .reasoning_ref_get(workspace, id, kind)
        .map_err(store_err("STORE_ERROR"))?;
    Ok(match existing {
        Some(row) => ReasoningRef {
            branch: row.branch,
            notes_doc: row.notes_doc,
            graph_doc: row.graph_doc,
            trace_doc: row.trace_doc,
        },
        None => ReasoningRef::for_entity(kind, id),
    })
}

fn doc_tail_json(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
    cursor: Option<i64>,
) -> Result<Value, SnapshotError> {
    let slice = store
        .doc_show_tail(workspace, branch, doc, cursor, MAX_DOC_TAIL_ENTRIES)
        .map_err(store_err("STORE_ERROR"))?;

    let entries = slice
        .entries
        .into_iter()
        .map(|entry| {
            let kind = match entry.kind {
                bm_storage::DocEntryKind::Note => "note",
                bm_storage::DocEntryKind::Event => "event",
            };
            json!({
                "seq": entry.seq,
                "ts_ms": entry.ts_ms,
                "kind": kind,
                "title": entry.title,
                "format": entry.format,
                "content": entry.content.map(|content| truncate_chars(&content, MAX_ENTRY_CONTENT_CHARS)),
                "event_type": entry.event_type,
                "task_id": entry.task_id,
                "path": entry.path
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "branch": branch,
        "doc": doc,
        "entries": entries,
        "has_more": slice.has_more,
        "next_cursor": slice.next_cursor
    }))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut iter = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match iter.next() {
            Some(ch) => out.push(ch),
            None => return value.to_string(),
        }
    }
    out.push('…');
    out
}

fn is_valid_plan_id(value: &str) -> bool {
    if !value.starts_with("PLAN-") {
        return false;
    }
    let suffix = &value["PLAN-".len()..];
    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
}

fn is_valid_task_id(value: &str) -> bool {
    if !value.starts_with("TASK-") {
        return false;
    }
    let suffix = &value["TASK-".len()..];
    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_storage::{DocAppendRequest, NewStep, TaskCreateRequest};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bm_viewer_detail_test_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn default_config(dir: &Path) -> ViewerConfig {
        ViewerConfig {
            storage_dir: dir.to_path_buf(),
            workspace: Some("demo".to_string()),
            project_guard: None,
            port: 0,
            runner_autostart_enabled: None,
            runner_autostart_dry_run: false,
            runner_autostart: None,
        }
    }

    #[test]
    fn task_detail_includes_steps_and_trace_tail() {
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

        let (task_id, _, _) = store
            .create(
                &workspace,
                TaskCreateRequest {
                    kind: TaskKind::Task,
                    title: "Task One".to_string(),
                    parent_plan_id: Some(plan_id.clone()),
                    description: Some("Do the thing.".to_string()),
                    contract: None,
                    contract_json: None,
                    event_type: "task_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .unwrap();

        store
            .steps_decompose(
                &workspace,
                &task_id,
                None,
                None,
                vec![NewStep {
                    title: "Step A".to_string(),
                    success_criteria: vec!["It works".to_string()],
                }],
            )
            .unwrap();

        let reasoning = ReasoningRef::for_entity(TaskKind::Task, &task_id);
        let _ = store
            .doc_append_trace(
                &workspace,
                DocAppendRequest {
                    branch: reasoning.branch.clone(),
                    doc: reasoning.trace_doc.clone(),
                    title: Some("update".to_string()),
                    format: None,
                    meta_json: None,
                    content: "Implemented a slice.".to_string(),
                },
            )
            .unwrap();

        let config = default_config(&dir);
        let detail = build_task_detail(&mut store, &config, None, &task_id, None, None).unwrap();
        let steps = detail
            .get("steps")
            .and_then(|v| v.get("items"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(steps.len(), 1);
        let tail = detail
            .get("trace_tail")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(!tail.is_empty());
        assert!(
            tail.iter()
                .any(|entry| entry.get("title").and_then(|v| v.as_str()) == Some("update")),
            "Expected trace_tail to include the appended 'update' entry"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_detail_includes_notes_tail() {
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
                    description: Some("Plan text.".to_string()),
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: "{}".to_string(),
                },
            )
            .unwrap();

        let reasoning = ReasoningRef::for_entity(TaskKind::Plan, &plan_id);
        let _ = store
            .doc_append_note(
                &workspace,
                DocAppendRequest {
                    branch: reasoning.branch.clone(),
                    doc: reasoning.notes_doc.clone(),
                    title: Some("note".to_string()),
                    format: None,
                    meta_json: None,
                    content: "This plan has a note.".to_string(),
                },
            )
            .unwrap();

        let config = default_config(&dir);
        let detail = build_plan_detail(&mut store, &config, None, &plan_id, None, None).unwrap();
        let notes = detail
            .get("notes_tail")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].get("title").and_then(|v| v.as_str()), Some("note"));

        let _ = fs::remove_dir_all(&dir);
    }
}
