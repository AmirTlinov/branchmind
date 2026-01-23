#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

const MINDPACK_DOC: &str = "mindpack";
const MINDPACK_VERSION: u64 = 1;
const MAX_CHANGED_LINES: usize = 5;
const MAX_MINDPACK_LINE_CHARS: usize = 200;
const MAX_MINDPACK_CONTENT_CHARS: usize = 1600;

fn mindpack_ref(seq: i64) -> String {
    format!("{MINDPACK_DOC}@{seq}")
}

fn parse_meta_json(meta_json: Option<&str>) -> Option<Value> {
    let raw = meta_json?.trim();
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(raw).ok()
}

fn extract_meta_str(meta: &Value, path: &[&str]) -> Option<String> {
    let mut cur = meta;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_meta_u64(meta: &Value, path: &[&str]) -> Option<u64> {
    let mut cur = meta;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_u64()
}

fn compute_changed_lines(prev_meta: Option<&Value>, next_meta: &Value) -> Vec<String> {
    let Some(prev) = prev_meta else {
        return Vec::new();
    };

    let mut out = Vec::<String>::new();
    let prev_focus = extract_meta_str(prev, &["focus", "id"]);
    let next_focus = extract_meta_str(next_meta, &["focus", "id"]);
    if prev_focus != next_focus
        && let Some(next_focus) = next_focus
    {
        out.push(format!("focus -> {next_focus}"));
    }

    let prev_where = extract_meta_str(prev, &["where", "where"]);
    let next_where = extract_meta_str(next_meta, &["where", "where"]);
    if prev_where != next_where
        && let Some(next_where) = next_where
        && next_where != "unknown"
    {
        out.push(format!("where -> {next_where}"));
    }

    let prev_active = extract_meta_u64(prev, &["horizon", "active"]);
    let next_active = extract_meta_u64(next_meta, &["horizon", "active"]);
    if prev_active != next_active
        && let (Some(a), Some(b)) = (prev_active, next_active)
    {
        out.push(format!("horizon.active {a} -> {b}"));
    }

    let prev_missing = extract_meta_u64(prev, &["kpi", "active_missing_anchor"]);
    let next_missing = extract_meta_u64(next_meta, &["kpi", "active_missing_anchor"]);
    if prev_missing != next_missing
        && let (Some(a), Some(b)) = (prev_missing, next_missing)
    {
        out.push(format!("kpi.missing_anchor {a} -> {b}"));
    }

    out.truncate(MAX_CHANGED_LINES);
    out
}

fn clamp_line(value: &str) -> String {
    truncate_string(&redact_text(value.trim()), MAX_MINDPACK_LINE_CHARS)
}

fn clamp_anchor_list(anchors: Vec<String>) -> Vec<String> {
    anchors
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| a.starts_with("a:") && !a.is_empty())
        .take(3)
        .collect()
}

fn format_mindpack_content(meta: &Value, changed: &[String]) -> String {
    let focus = extract_meta_str(meta, &["focus", "id"]).unwrap_or_else(|| "none".to_string());
    let where_id =
        extract_meta_str(meta, &["where", "where"]).unwrap_or_else(|| "unknown".to_string());
    let plan = extract_meta_str(meta, &["plan", "id"]).unwrap_or_else(|| "-".to_string());
    let missing_anchor = extract_meta_u64(meta, &["kpi", "active_missing_anchor"]).unwrap_or(0);
    let active_total = extract_meta_u64(meta, &["kpi", "active_total"]).unwrap_or(0);

    let mut lines = Vec::<String>::new();
    lines.push(clamp_line(&format!(
        "mindpack v{MINDPACK_VERSION} | focus={focus} | where={where_id} | plan={plan} | kpi missing_anchor={missing_anchor}/{active_total}"
    )));

    if let Some(h) = meta.get("horizon").and_then(|v| v.as_object()) {
        let active = h.get("active").and_then(|v| v.as_u64()).unwrap_or(0);
        let backlog = h.get("backlog").and_then(|v| v.as_u64()).unwrap_or(0);
        let done = h.get("done").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = h
            .get("total")
            .and_then(|v| v.as_u64())
            .unwrap_or(active + backlog + done);
        lines.push(clamp_line(&format!(
            "horizon active={active} backlog={backlog} done={done} total={total}"
        )));
    }

    if let Some(arr) = meta
        .get("where")
        .and_then(|v| v.get("top_anchors"))
        .and_then(|v| v.as_array())
    {
        let anchors = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        let anchors = clamp_anchor_list(anchors);
        if !anchors.is_empty() {
            lines.push(clamp_line(&format!("anchors {}", anchors.join(" "))));
        }
    }

    if let Some(next_tool) = extract_meta_str(meta, &["next", "tool"]) {
        lines.push(clamp_line(&format!("next {next_tool}")));
    }
    if let Some(backup_tool) = extract_meta_str(meta, &["backup", "tool"]) {
        lines.push(clamp_line(&format!("backup {backup_tool}")));
    }

    if !changed.is_empty() {
        lines.push("changed:".to_string());
        for line in changed.iter().take(MAX_CHANGED_LINES) {
            let value = clamp_line(line);
            if !value.is_empty() {
                lines.push(format!("- {value}"));
            }
        }
    }

    let mut out = lines.join("\n");
    if out.len() > MAX_MINDPACK_CONTENT_CHARS {
        out.truncate(MAX_MINDPACK_CONTENT_CHARS);
        out.push_str("\n…");
    }
    out
}

pub(crate) struct MindpackEnsureResult {
    pub(crate) ref_id: Option<String>,
    pub(crate) doc: String,
    pub(crate) seq: Option<i64>,
    pub(crate) ts_ms: Option<i64>,
    pub(crate) summary: String,
    pub(crate) meta: Value,
    pub(crate) updated: bool,
    pub(crate) changed: Vec<String>,
}

pub(crate) fn ensure_mindpack(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    update: bool,
    reason: Option<String>,
    read_only: bool,
) -> Result<MindpackEnsureResult, StoreError> {
    let checkout = server
        .store
        .branch_checkout_get(workspace)?
        .ok_or(StoreError::InvalidInput("workspace has no checkout branch"))?;

    let previous = server
        .store
        .doc_show_tail(workspace, &checkout, MINDPACK_DOC, None, 1)
        .ok()
        .and_then(|slice| slice.entries.last().cloned());
    let previous_meta = previous
        .as_ref()
        .and_then(|e| parse_meta_json(e.meta_json.as_deref()));
    let previous_content = previous
        .as_ref()
        .and_then(|e| e.content.as_deref())
        .map(|s| s.to_string());

    let focus_id = server.store.focus_get(workspace).ok().flatten();
    let mut focus_title: Option<String> = None;
    let mut plan_id: Option<String> = None;
    let focus_kind = focus_id.as_deref().and_then(|id| {
        if id.starts_with("TASK-") {
            Some("task")
        } else if id.starts_with("PLAN-") {
            Some("plan")
        } else {
            None
        }
    });

    if let Some(focus_id) = focus_id.as_deref() {
        if focus_id.starts_with("TASK-") {
            if let Ok(Some(task)) = server.store.get_task(workspace, focus_id) {
                focus_title = Some(clamp_line(&task.title));
                plan_id = Some(task.parent_plan_id);
            }
        } else if focus_id.starts_with("PLAN-")
            && let Ok(Some(plan)) = server.store.get_plan(workspace, focus_id)
        {
            focus_title = Some(clamp_line(&plan.title));
            plan_id = Some(plan.id);
        }
    }

    const DEFAULT_TASK_STALE_AFTER_MS: i64 = 14 * 24 * 60 * 60 * 1000;
    let horizon = plan_id
        .as_deref()
        .filter(|id| id.starts_with("PLAN-"))
        .and_then(|plan_id| {
            server
                .store
                .plan_horizon_stats_for_plan(
                    workspace,
                    plan_id,
                    crate::support::now_ms_i64(),
                    DEFAULT_TASK_STALE_AFTER_MS,
                )
                .ok()
        })
        .map(|stats| {
            let active = stats.active.max(0) as u64;
            let backlog = stats.backlog.max(0) as u64;
            let parked = stats.parked.max(0) as u64;
            let stale = stats.stale.max(0) as u64;
            let done = stats.done.max(0) as u64;
            let total = stats.total.max(0) as u64;

            let mut out = json!({
                "active": active,
                "backlog": backlog,
                "parked": parked,
                "stale": stale,
                "done": done,
                "total": total,
                "active_limit": 3u64,
                "over_active_limit": active > 3
            });
            if let Some(wake) = stats.next_wake
                && let Some(obj) = out.as_object_mut()
            {
                obj.insert(
                    "next_wake".to_string(),
                    json!({
                        "task": wake.task_id,
                        "parked_until_ts_ms": wake.parked_until_ts_ms
                    }),
                );
            }
            out
        })
        .unwrap_or(Value::Null);

    // Where/top_anchors: prefer focused task anchors; fall back to plan ACTIVE anchors.
    let mut top_anchors: Vec<String> = Vec::new();
    let mut where_id: String = "unknown".to_string();

    let plan_coverage = plan_id
        .as_deref()
        .filter(|id| id.starts_with("PLAN-"))
        .and_then(|plan_id| {
            server
                .store
                .plan_anchors_coverage(
                    workspace,
                    bm_storage::PlanAnchorsCoverageRequest {
                        plan_id: plan_id.to_string(),
                        top_anchors_limit: 3,
                    },
                )
                .ok()
        });

    let (kpi_active_total, kpi_missing_anchor, kpi_checked, kpi_partial) =
        if let Some(coverage) = plan_coverage.as_ref() {
            (
                coverage.active_total,
                coverage.active_missing_anchor,
                coverage.active_total,
                false,
            )
        } else {
            // We can still report how many ACTIVE tasks exist, but coverage is unknown without
            // scanning, so KPIs are partial.
            (
                horizon.get("active").and_then(|v| v.as_u64()).unwrap_or(0),
                0,
                0,
                true,
            )
        };

    if let Some(focus_id) = focus_id.as_deref().filter(|id| id.starts_with("TASK-")) {
        if let Ok(list) = server.store.task_anchors_list(
            workspace,
            bm_storage::TaskAnchorsListRequest {
                task_id: focus_id.to_string(),
                limit: 3,
            },
        ) {
            top_anchors = list.anchors.into_iter().map(|h| h.anchor_id).collect();
            top_anchors = clamp_anchor_list(top_anchors);
            where_id = top_anchors
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
        }
    } else if let Some(coverage) = plan_coverage.as_ref() {
        top_anchors = coverage
            .top_anchors
            .iter()
            .map(|h| h.anchor_id.clone())
            .collect();
        top_anchors = clamp_anchor_list(top_anchors);
        where_id = top_anchors
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
    }

    let mut meta = json!({
        "version": MINDPACK_VERSION,
        "reason": reason.map(|v| clamp_line(&v)).unwrap_or_default(),
        "focus": {
            "id": focus_id.clone().unwrap_or_else(|| "none".to_string()),
            "kind": focus_kind.unwrap_or("none"),
            "title": focus_title.map(Value::String).unwrap_or(Value::Null)
        },
        "plan": {
            "id": plan_id.clone().unwrap_or_else(|| "-".to_string())
        },
        "where": {
            "where": where_id.clone(),
            "top_anchors": top_anchors.clone()
        },
        "horizon": horizon,
        "kpi": {
            "active_total": kpi_active_total,
            "active_checked": kpi_checked,
            "active_missing_anchor": kpi_missing_anchor,
            "partial": kpi_partial
        },
        "next": { "tool": "tasks_snapshot" },
        "backup": {
            "tool": "tasks_lint",
            "args_hint": if let Some(focus_id) = focus_id.as_deref() {
                if focus_id.starts_with("TASK-") {
                    json!({ "task": focus_id })
                } else if focus_id.starts_with("PLAN-") {
                    json!({ "plan": focus_id })
                } else {
                    Value::Null
                }
            } else {
                Value::Null
            }
        }
    });

    if let Some(obj) = meta.as_object_mut()
        && obj
            .get("reason")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.is_empty())
    {
        obj.remove("reason");
    }

    let changed = compute_changed_lines(previous_meta.as_ref(), &meta);
    let content = format_mindpack_content(&meta, &changed);
    let summary = content
        .lines()
        .next()
        .unwrap_or("mindpack")
        .trim()
        .to_string();

    // If we are not updating (or we must be read-only), return the best available stored
    // navigation handle (latest entry) and include computed meta/summary for visibility.
    if !update || read_only {
        if let Some(prev) = previous {
            let ref_id = Some(mindpack_ref(prev.seq));
            let meta = previous_meta.unwrap_or(meta);
            let summary = prev
                .content
                .as_deref()
                .and_then(|c| c.lines().next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or(summary);
            return Ok(MindpackEnsureResult {
                ref_id,
                doc: MINDPACK_DOC.to_string(),
                seq: Some(prev.seq),
                ts_ms: Some(prev.ts_ms),
                summary,
                meta,
                updated: false,
                changed: Vec::new(),
            });
        }

        return Ok(MindpackEnsureResult {
            ref_id: None,
            doc: MINDPACK_DOC.to_string(),
            seq: None,
            ts_ms: None,
            summary,
            meta,
            updated: false,
            changed,
        });
    }

    // Update mode: append only if the *stable state* changed.
    //
    // Important: `changed[]` is derived from `(previous_meta, meta)` and is intended as a
    // “delta hint” for humans. It must not force an extra append on the next snapshot when the
    // state is otherwise identical (that would spam `mindpack@seq` versions after every change).
    fn meta_for_dedupe(meta: &Value) -> Value {
        let mut out = meta.clone();
        if let Some(obj) = out.as_object_mut() {
            // Reason is a useful tag, but it is not a state change worth appending a new version.
            obj.remove("reason");
        }
        out
    }

    let needs_append = if let Some(prev_meta) = previous_meta.as_ref() {
        meta_for_dedupe(prev_meta) != meta_for_dedupe(&meta)
    } else {
        match previous_content.as_deref() {
            Some(prev) => prev != content.as_str(),
            None => true,
        }
    };
    if !needs_append && let Some(prev) = previous {
        return Ok(MindpackEnsureResult {
            ref_id: Some(mindpack_ref(prev.seq)),
            doc: MINDPACK_DOC.to_string(),
            seq: Some(prev.seq),
            ts_ms: Some(prev.ts_ms),
            summary,
            meta: previous_meta.unwrap_or(meta),
            updated: false,
            changed: Vec::new(),
        });
    }

    let meta_json = serde_json::to_string(&meta).ok();
    let appended = server.store.doc_append_note(
        workspace,
        bm_storage::DocAppendRequest {
            branch: checkout,
            doc: MINDPACK_DOC.to_string(),
            title: Some(format!("mindpack v{MINDPACK_VERSION}")),
            format: Some("text/plain".to_string()),
            meta_json,
            content,
        },
    )?;

    Ok(MindpackEnsureResult {
        ref_id: Some(mindpack_ref(appended.seq)),
        doc: MINDPACK_DOC.to_string(),
        seq: Some(appended.seq),
        ts_ms: Some(appended.ts_ms),
        summary,
        meta,
        updated: true,
        changed,
    })
}

impl McpServer {
    pub(crate) fn tool_tasks_mindpack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let update = args_obj
            .get("update")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let reason = args_obj
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let ensured = match ensure_mindpack(self, &workspace, update, reason, read_only) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "mindpack": {
                "ref": ensured.ref_id,
                "doc": ensured.doc,
                "seq": ensured.seq,
                "ts_ms": ensured.ts_ms,
                "summary": ensured.summary,
                "meta": ensured.meta
            },
            "updated": ensured.updated,
            "changed": ensured.changed
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let (_used, trimmed_fields) = enforce_max_chars_budget(&mut result, limit);
            truncated |= trimmed_fields;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |_value| {
                    false
                });
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("mindpack", result)
        } else {
            ai_ok_with_warnings("mindpack", result, warnings, Vec::new())
        }
    }
}
