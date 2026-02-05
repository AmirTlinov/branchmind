#![forbid(unsafe_code)]

use super::ViewerConfig;
use super::knowledge_snapshot::knowledge_key_task_id;
use super::snapshot::{SnapshotError, store_err};
use crate::{now_ms_i64, now_rfc3339};
use bm_core::ids::WorkspaceId;
use bm_storage::{
    AnchorsListRequest, KnowledgeKeysSearchRequest, PlansSearchRequest, SqliteStore,
    TasksSearchRequest,
};
use serde_json::{Value, json};

const DEFAULT_LIMIT: usize = 60;
const MAX_LIMIT: usize = 120;
const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone, Debug)]
struct SearchItem {
    kind: &'static str,
    id: String,
    title: String,
    plan_id: String,
    updated_at_ms: i64,
    extra: Option<Value>,
}

fn score_item(needle: &str, item: &SearchItem) -> i64 {
    if needle.is_empty() {
        return 0;
    }
    let id = item.id.to_ascii_lowercase();
    let title = item.title.to_ascii_lowercase();
    if id == needle {
        return -10;
    }
    if id.starts_with(needle) {
        return -6;
    }
    if id.contains(needle) {
        return -4;
    }
    if title.contains(needle) {
        return -2;
    }
    10
}

fn kind_bias(kind: &str) -> i64 {
    match kind {
        "plan" | "anchor" => -1,
        _ => 0,
    }
}

pub(crate) fn build_search(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    lens: &str,
    query: &str,
    limit_override: Option<usize>,
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
        return Ok(empty_search(
            lens,
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
                guard_status = "uninitialized";
            }
        }
    }

    let limit = limit_override.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let query = query.trim();
    if query.is_empty() {
        return Ok(json!({
            "generated_at": generated_at,
            "generated_at_ms": generated_at_ms,
            "lens": lens,
            "workspace": workspace.as_str(),
            "workspace_exists": true,
            "project_guard": {
                "expected": expected_guard,
                "stored": stored_guard.as_deref(),
                "status": guard_status
            },
            "query": "",
            "limit": limit,
            "items": [],
            "has_more": false
        }));
    }
    let query = if query.len() > MAX_QUERY_CHARS {
        &query[..MAX_QUERY_CHARS]
    } else {
        query
    };

    let needle = query.to_ascii_lowercase();

    let mut items = Vec::<SearchItem>::new();
    let mut has_more = false;

    match lens {
        "knowledge" => {
            let anchors = store
                .anchors_list(
                    &workspace,
                    AnchorsListRequest {
                        text: Some(query.to_string()),
                        kind: None,
                        status: None,
                        limit,
                    },
                )
                .map_err(store_err("STORE_ERROR"))?;
            if anchors.has_more {
                has_more = true;
            }
            for anchor in anchors.anchors.into_iter() {
                items.push(SearchItem {
                    kind: "anchor",
                    id: anchor.id.clone(),
                    title: anchor.title.clone(),
                    plan_id: anchor.id.clone(),
                    updated_at_ms: anchor.updated_at_ms,
                    extra: Some(json!({
                        "anchor_id": anchor.id,
                        "anchor_kind": anchor.kind,
                        "anchor_status": anchor.status
                    })),
                });
            }

            let keys = store
                .knowledge_keys_search(
                    &workspace,
                    KnowledgeKeysSearchRequest {
                        text: query.to_string(),
                        limit,
                    },
                )
                .map_err(store_err("STORE_ERROR"))?;
            if keys.has_more {
                has_more = true;
            }
            for row in keys.items.into_iter() {
                let id = knowledge_key_task_id(&row.anchor_id, &row.key);
                items.push(SearchItem {
                    kind: "knowledge_key",
                    id,
                    title: row.key.clone(),
                    plan_id: row.anchor_id.clone(),
                    updated_at_ms: row.updated_at_ms,
                    extra: Some(json!({
                        "anchor_id": row.anchor_id,
                        "key": row.key,
                        "card_id": row.card_id
                    })),
                });
            }
        }
        _ => {
            let plans = store
                .search_plans(
                    &workspace,
                    PlansSearchRequest {
                        text: query.to_string(),
                        limit,
                    },
                )
                .map_err(store_err("STORE_ERROR"))?;
            if plans.has_more {
                has_more = true;
            }
            for plan in plans.plans.into_iter() {
                items.push(SearchItem {
                    kind: "plan",
                    id: plan.id.clone(),
                    title: plan.title.clone(),
                    plan_id: plan.id,
                    updated_at_ms: plan.updated_at_ms,
                    extra: None,
                });
            }

            let tasks = store
                .search_tasks(
                    &workspace,
                    TasksSearchRequest {
                        text: query.to_string(),
                        limit,
                    },
                )
                .map_err(store_err("STORE_ERROR"))?;
            if tasks.has_more {
                has_more = true;
            }
            for task in tasks.tasks.into_iter() {
                items.push(SearchItem {
                    kind: "task",
                    id: task.id.clone(),
                    title: task.title.clone(),
                    plan_id: task.plan_id,
                    updated_at_ms: task.updated_at_ms,
                    extra: None,
                });
            }
        }
    }

    items.sort_by(|a, b| {
        let diff = score_item(&needle, a).cmp(&score_item(&needle, b));
        if diff != std::cmp::Ordering::Equal {
            return diff;
        }
        let bias = kind_bias(a.kind).cmp(&kind_bias(b.kind));
        if bias != std::cmp::Ordering::Equal {
            return bias;
        }
        let time = b.updated_at_ms.cmp(&a.updated_at_ms);
        if time != std::cmp::Ordering::Equal {
            return time;
        }
        a.id.cmp(&b.id)
    });

    if items.len() > limit {
        has_more = true;
        items.truncate(limit);
    }

    let json_items = items
        .into_iter()
        .map(|item| {
            let mut base = json!({
                "kind": item.kind,
                "id": item.id,
                "title": item.title,
                "plan_id": item.plan_id
            });
            if let Some(extra) = item.extra
                && let Some(obj) = base.as_object_mut()
                && let Some(extra_obj) = extra.as_object()
            {
                for (k, v) in extra_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
            base
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "lens": lens,
        "workspace": workspace.as_str(),
        "workspace_exists": true,
        "project_guard": {
            "expected": expected_guard,
            "stored": stored_guard.as_deref(),
            "status": guard_status
        },
        "query": query,
        "limit": limit,
        "items": json_items,
        "has_more": has_more
    }))
}

fn empty_search(
    lens: &str,
    workspace: &str,
    generated_at: Value,
    generated_at_ms: i64,
    expected_guard: Option<&str>,
) -> Value {
    json!({
        "generated_at": generated_at,
        "generated_at_ms": generated_at_ms,
        "lens": lens,
        "workspace": workspace,
        "workspace_exists": false,
        "project_guard": {
            "expected": expected_guard,
            "stored": Value::Null,
            "status": if expected_guard.is_some() { "unknown" } else { "not_applicable" }
        },
        "query": "",
        "limit": DEFAULT_LIMIT,
        "items": [],
        "has_more": false
    })
}
