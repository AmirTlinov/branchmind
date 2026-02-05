#![forbid(unsafe_code)]

use super::ViewerConfig;
use super::snapshot::{SnapshotError, store_err};
use crate::{now_ms_i64, now_rfc3339};
use bm_core::ids::WorkspaceId;
use bm_storage::{PlanRow, SqliteStore, TaskRow, TasksListForPlanCursorRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 600;

const CLUSTER_TILE: f64 = 0.45;
const CLUSTER_SCAN_CHUNK: usize = 420;
const CLUSTER_MAX_SCAN: usize = 5000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalGraphRequest<'a> {
    pub lens: &'a str,
    pub node_id: &'a str,
    pub hops: u8,
    pub cursor: Option<&'a str>,
    pub limit: Option<usize>,
}

pub(crate) fn normalize_graph_id_param(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 96 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '_' | '-'))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn ensure_workspace_and_guard(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
) -> Result<(WorkspaceId, Option<String>, &'static str), SnapshotError> {
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
        return Err(SnapshotError {
            code: "WORKSPACE_NOT_FOUND",
            message: "workspace: not found.".to_string(),
            recovery: Some("Pick a workspace from /api/workspaces.".to_string()),
            status: 404,
        });
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
                guard_status = "uninitialized";
            }
        }
    }

    Ok((workspace, stored_guard, guard_status))
}

fn task_summary(task: &TaskRow) -> Value {
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
}

fn plan_counts(store_counts: BTreeMap<String, i64>) -> Value {
    let mut total: i64 = 0;
    let mut done: i64 = 0;
    let mut parked: i64 = 0;
    let mut backlog: i64 = 0;
    let mut active: i64 = 0;
    for (status, count) in store_counts {
        total += count;
        match status.as_str() {
            "DONE" => done += count,
            "PARKED" => parked += count,
            "TODO" => backlog += count,
            _ => active += count,
        }
    }
    json!({
        "total": total,
        "done": done,
        "active": active,
        "backlog": backlog,
        "parked": parked
    })
}

fn plan_summary(plan: &PlanRow, counts: Value) -> Value {
    json!({
        "id": plan.id.clone(),
        "title": plan.title.clone(),
        "description": plan.description.clone(),
        "context": plan.context.clone(),
        "status": plan.status.clone(),
        "priority": plan.priority.clone(),
        "updated_at_ms": plan.updated_at_ms,
        "task_counts": counts
    })
}

pub(crate) fn build_plan_subgraph(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    lens: &str,
    plan_id: &str,
    cursor_raw: Option<&str>,
    limit_override: Option<usize>,
) -> Result<Value, SnapshotError> {
    if lens != "work" {
        return Err(SnapshotError {
            code: "INVALID_LENS",
            message: "lens: expected work.".to_string(),
            recovery: Some("Remove lens=... or pass lens=work.".to_string()),
            status: 400,
        });
    }

    let generated_at_ms = now_ms_i64();
    let generated_at = now_rfc3339();

    let (workspace, stored_guard, guard_status) =
        ensure_workspace_and_guard(store, config, workspace_override)?;

    let expected_guard = config.project_guard.as_deref();

    let plan_id = normalize_graph_id_param(plan_id).ok_or_else(|| SnapshotError {
        code: "INVALID_PLAN_ID",
        message: "plan_id: invalid.".to_string(),
        recovery: Some("Pick an existing PLAN id from /api/snapshot.".to_string()),
        status: 400,
    })?;

    let cursor = match cursor_raw {
        Some(raw) => normalize_graph_id_param(raw).ok_or_else(|| SnapshotError {
            code: "INVALID_CURSOR",
            message: "cursor: invalid.".to_string(),
            recovery: Some(
                "Omit cursor or pass a TASK id previously returned as next_cursor.".to_string(),
            ),
            status: 400,
        })?,
        None => String::new(),
    };
    let cursor = if cursor.is_empty() {
        None
    } else {
        Some(cursor)
    };

    let limit = limit_override.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let Some(plan) = store
        .get_plan(&workspace, &plan_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_PLAN",
            message: "plan_id: plan not found.".to_string(),
            recovery: Some("Pick an existing PLAN id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let counts_raw = store
        .count_tasks_by_status_for_plan(&workspace, &plan_id)
        .map_err(store_err("STORE_ERROR"))?;
    let counts = plan_counts(counts_raw);
    let tasks_total = counts.get("total").and_then(|v| v.as_i64()).unwrap_or(0);

    let page = store
        .list_tasks_for_plan_cursor(
            &workspace,
            TasksListForPlanCursorRequest {
                plan_id: plan_id.clone(),
                cursor,
                limit,
            },
        )
        .map_err(store_err("STORE_ERROR"))?;

    let tasks_json = page.tasks.iter().map(task_summary).collect::<Vec<_>>();

    Ok(json!({
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "lens": "work",
        "workspace": workspace.as_str(),
        "workspace_exists": true,
        "project_guard": {
            "expected": expected_guard,
            "stored": stored_guard.as_deref(),
            "status": guard_status
        },
        "plan": plan_summary(&plan, counts),
        "tasks_total": tasks_total,
        "tasks": tasks_json,
        "pagination": {
            "cursor": cursor_raw,
            "limit": limit,
            "has_more": page.has_more,
            "next_cursor": page.next_cursor
        }
    }))
}

fn parse_cluster_id(cluster_id: &str) -> Option<(String, i32, i32)> {
    let mut parts = cluster_id.split(':');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("C"), Some(plan_id), Some(tile_x), Some(tile_y), None) => {
            let plan_id = normalize_graph_id_param(plan_id)?;
            let tile_x = tile_x.parse::<i32>().ok()?;
            let tile_y = tile_y.parse::<i32>().ok()?;
            Some((plan_id, tile_x, tile_y))
        }
        _ => None,
    }
}

pub(crate) fn build_cluster_subgraph(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    lens: &str,
    cluster_id: &str,
    cursor_raw: Option<&str>,
    limit_override: Option<usize>,
) -> Result<Value, SnapshotError> {
    if lens != "work" {
        return Err(SnapshotError {
            code: "INVALID_LENS",
            message: "lens: expected work.".to_string(),
            recovery: Some("Remove lens=... or pass lens=work.".to_string()),
            status: 400,
        });
    }

    let generated_at_ms = now_ms_i64();
    let generated_at = now_rfc3339();

    let (workspace, stored_guard, guard_status) =
        ensure_workspace_and_guard(store, config, workspace_override)?;

    let expected_guard = config.project_guard.as_deref();

    let (plan_id, tile_x, tile_y) = parse_cluster_id(cluster_id).ok_or_else(|| SnapshotError {
        code: "INVALID_CLUSTER_ID",
        message: "cluster_id: expected C:<plan_id>:<tileX>:<tileY>.".to_string(),
        recovery: Some("Pick a cluster id from the graph UI (clusters LOD).".to_string()),
        status: 400,
    })?;

    let cursor = cursor_raw.and_then(normalize_graph_id_param);
    if cursor_raw.is_some() && cursor.is_none() {
        return Err(SnapshotError {
            code: "INVALID_CURSOR",
            message: "cursor: invalid.".to_string(),
            recovery: Some(
                "Omit cursor or pass a TASK id previously returned as next_cursor.".to_string(),
            ),
            status: 400,
        });
    }

    let limit = limit_override.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let Some(plan) = store
        .get_plan(&workspace, &plan_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_PLAN",
            message: "cluster_id: plan not found.".to_string(),
            recovery: Some("Pick an existing PLAN id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let mut out = Vec::<TaskRow>::new();
    let mut scan_cursor = cursor.clone();
    let mut scanned: usize = 0;
    let mut has_more = false;

    while out.len() < limit && scanned < CLUSTER_MAX_SCAN {
        let page = store
            .list_tasks_for_plan_cursor(
                &workspace,
                TasksListForPlanCursorRequest {
                    plan_id: plan_id.clone(),
                    cursor: scan_cursor.clone(),
                    limit: CLUSTER_SCAN_CHUNK,
                },
            )
            .map_err(store_err("STORE_ERROR"))?;

        if page.tasks.is_empty() {
            has_more = false;
            break;
        }

        let mut page_has_more = page.has_more;
        for task in page.tasks.into_iter() {
            scanned = scanned.saturating_add(1);
            scan_cursor = Some(task.id.clone());

            let tile = task_tile(&task);
            if tile.0 == tile_x && tile.1 == tile_y {
                out.push(task);
                if out.len() >= limit {
                    // Stop early but keep scan_cursor pointing at the last scanned id so the next
                    // request can continue without skipping the remaining rows from this page.
                    has_more = true;
                    page_has_more = true;
                    break;
                }
            }

            if scanned >= CLUSTER_MAX_SCAN {
                has_more = true;
                page_has_more = true;
                break;
            }
        }

        if out.len() >= limit || scanned >= CLUSTER_MAX_SCAN {
            break;
        }

        if !page_has_more {
            has_more = false;
            scan_cursor = None;
            break;
        }
        // Continue scanning.
        has_more = true;
        if scan_cursor.is_none() {
            break;
        }
    }

    let tasks_json = out.iter().map(task_summary).collect::<Vec<_>>();

    Ok(json!({
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "lens": "work",
        "workspace": workspace.as_str(),
        "workspace_exists": true,
        "project_guard": {
            "expected": expected_guard,
            "stored": stored_guard.as_deref(),
            "status": guard_status
        },
        "plan_id": plan_id,
        "plan": plan_summary(&plan, json!({"total": 0, "done": 0, "active": 0, "backlog": 0, "parked": 0})),
        "cluster_id": cluster_id,
        "tile": {
            "tile": CLUSTER_TILE,
            "x": tile_x,
            "y": tile_y
        },
        "tasks": tasks_json,
        "pagination": {
            "cursor": cursor_raw,
            "limit": limit,
            "has_more": has_more,
            "next_cursor": if has_more { scan_cursor } else { None }
        }
    }))
}

pub(crate) fn build_local_graph(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    request: LocalGraphRequest<'_>,
) -> Result<Value, SnapshotError> {
    let lens = request.lens;
    let node_id = request.node_id;
    let hops = request.hops;
    let cursor_raw = request.cursor;
    let limit_override = request.limit;

    if lens != "work" {
        return Err(SnapshotError {
            code: "INVALID_LENS",
            message: "lens: expected work.".to_string(),
            recovery: Some("Remove lens=... or pass lens=work.".to_string()),
            status: 400,
        });
    }

    let generated_at_ms = now_ms_i64();
    let generated_at = now_rfc3339();

    let (workspace, stored_guard, guard_status) =
        ensure_workspace_and_guard(store, config, workspace_override)?;
    let expected_guard = config.project_guard.as_deref();

    let limit = limit_override.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let hops = hops.clamp(1, 2);

    let id = normalize_graph_id_param(node_id).ok_or_else(|| SnapshotError {
        code: "INVALID_NODE_ID",
        message: "node_id: invalid.".to_string(),
        recovery: Some("Pick an id from /api/search or /api/snapshot.".to_string()),
        status: 400,
    })?;

    if id.starts_with("PLAN-") {
        let plan = build_plan_subgraph(
            store,
            config,
            Some(workspace.as_str()),
            lens,
            &id,
            cursor_raw,
            Some(limit),
        )?;
        return Ok(json!({
            "generated_at": generated_at,
            "generated_at_ms": generated_at_ms,
            "lens": "work",
            "workspace": workspace.as_str(),
            "workspace_exists": true,
            "project_guard": {
                "expected": expected_guard,
                "stored": stored_guard.as_deref(),
                "status": guard_status
            },
            "root": { "kind": "plan", "id": id },
            "plan": plan.get("plan").cloned().unwrap_or(Value::Null),
            "tasks": plan.get("tasks").cloned().unwrap_or_else(|| json!([])),
            "pagination": plan.get("pagination").cloned().unwrap_or(Value::Null)
        }));
    }

    if !id.starts_with("TASK-") {
        return Err(SnapshotError {
            code: "UNSUPPORTED_NODE",
            message: "node_id: expected PLAN-* or TASK-*.".to_string(),
            recovery: Some("Use /api/search to locate a PLAN or TASK id.".to_string()),
            status: 400,
        });
    }

    let Some(task) = store
        .get_task(&workspace, &id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_TASK",
            message: "node_id: task not found.".to_string(),
            recovery: Some("Pick an existing TASK id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let Some(plan) = store
        .get_plan(&workspace, &task.parent_plan_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_PLAN",
            message: "task.plan_id: plan not found.".to_string(),
            recovery: Some("Pick an existing PLAN id from /api/snapshot.".to_string()),
            status: 404,
        });
    };

    let tile = task_tile(&task);
    let cluster_id = format!("C:{}:{}:{}", task.parent_plan_id, tile.0, tile.1);

    let cursor = cursor_raw.and_then(normalize_graph_id_param);
    if cursor_raw.is_some() && cursor.is_none() {
        return Err(SnapshotError {
            code: "INVALID_CURSOR",
            message: "cursor: invalid.".to_string(),
            recovery: Some(
                "Omit cursor or pass a TASK id previously returned as next_cursor.".to_string(),
            ),
            status: 400,
        });
    }

    let (mut neighbors, pagination) = if hops >= 2 {
        let neighbor_limit = limit.saturating_sub(1).max(1);
        let cluster = build_cluster_subgraph(
            store,
            config,
            Some(workspace.as_str()),
            lens,
            &cluster_id,
            cursor.as_deref(),
            Some(neighbor_limit),
        )?;
        (
            cluster
                .get("tasks")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            cluster.get("pagination").cloned().unwrap_or(Value::Null),
        )
    } else {
        (Vec::new(), Value::Null)
    };

    let root_task = task_summary(&task);
    neighbors.retain(|item| item.get("id").and_then(|v| v.as_str()) != Some(task.id.as_str()));
    let mut tasks = Vec::<Value>::with_capacity(neighbors.len() + 1);
    tasks.push(root_task);
    tasks.extend(neighbors);

    Ok(json!({
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "lens": "work",
        "workspace": workspace.as_str(),
        "workspace_exists": true,
        "project_guard": {
            "expected": expected_guard,
            "stored": stored_guard.as_deref(),
            "status": guard_status
        },
        "root": { "kind": "task", "id": task.id.clone(), "plan_id": task.parent_plan_id.clone() },
        "plan": plan_summary(&plan, json!({"total": 0, "done": 0, "active": 0, "backlog": 0, "parked": 0})),
        "cluster_id": cluster_id,
        "tasks": tasks,
        "pagination": pagination
    }))
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "to"
            | "of"
            | "in"
            | "on"
            | "a"
            | "an"
            | "or"
            | "via"
            | "as"
            | "is"
            | "are"
            | "be"
            | "by"
            | "from"
            | "at"
            | "it"
            | "this"
            | "that"
            | "these"
            | "those"
            | "plan"
            | "task"
            | "steps"
            | "step"
            | "mcp"
            | "dx"
            | "v1"
            | "v2"
            | "bm"
            | "ii"
            | "iii"
            | "и"
            | "для"
            | "что"
            | "это"
            | "на"
            | "в"
            | "по"
            | "из"
            | "к"
            | "с"
            | "без"
            | "или"
            | "а"
    )
}

fn fnv1a32_utf16(s: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for unit in s.encode_utf16() {
        hash ^= unit as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn fnv1a32_utf16_suffix(prefix: &str, suffix: &str) -> u32 {
    let mut hash = fnv1a32_utf16(prefix);
    for unit in suffix.encode_utf16() {
        hash ^= unit as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn hash_to_unit(hash: u32) -> f64 {
    (hash as f64) / 4294967296.0
}

fn semantic_vector(text: &str, fallback_id: &str) -> (f64, f64) {
    let raw = text.to_lowercase();

    let mut sum_x: f64 = 0.0;
    let mut sum_y: f64 = 0.0;
    let mut token_count: usize = 0;

    let mut buf = String::new();
    let flush = |buf: &mut String, sum_x: &mut f64, sum_y: &mut f64, token_count: &mut usize| {
        if buf.is_empty() {
            return;
        }
        let unit_len = buf.encode_utf16().count();
        if unit_len >= 3 && !is_stop_word(buf.as_str()) {
            *token_count = token_count.saturating_add(1);
            *sum_x += hash_to_unit(fnv1a32_utf16_suffix(buf.as_str(), "|x")) * 2.0 - 1.0;
            *sum_y += hash_to_unit(fnv1a32_utf16_suffix(buf.as_str(), "|y")) * 2.0 - 1.0;
        }
        buf.clear();
    };

    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            buf.push(ch);
        } else {
            flush(&mut buf, &mut sum_x, &mut sum_y, &mut token_count);
        }
    }
    flush(&mut buf, &mut sum_x, &mut sum_y, &mut token_count);

    let scale = (token_count.max(1) as f64).sqrt();
    let mut x = (sum_x / scale).tanh();
    let mut y = (sum_y / scale).tanh();
    let len = x.hypot(y);
    if len < 1e-3 {
        let seed = hash_to_unit(fnv1a32_utf16(fallback_id));
        let angle = seed * std::f64::consts::PI * 2.0;
        x = angle.cos() * 0.35;
        y = angle.sin() * 0.35;
    }
    (x, y)
}

fn task_tile(task: &TaskRow) -> (i32, i32) {
    let title = task.title.trim();
    let title = if title.is_empty() {
        task.id.as_str()
    } else {
        title
    };
    let desc = task.description.as_deref().unwrap_or("");
    let text = format!("{title} {desc}");
    let vec = semantic_vector(&text, task.id.as_str());
    let tile_x = ((vec.0 + 1.0) / CLUSTER_TILE).floor() as i32;
    let tile_y = ((vec.1 + 1.0) / CLUSTER_TILE).floor() as i32;
    (tile_x, tile_y)
}
