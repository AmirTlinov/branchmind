#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

fn delta_summary_one_line(title: Option<&str>, text: Option<&str>, max_len: usize) -> String {
    let title = title.unwrap_or("").trim();
    if !title.is_empty() {
        return truncate_string(&redact_text(title), max_len);
    }
    let text = text.unwrap_or("").trim();
    if text.is_empty() {
        return String::new();
    }
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or(text);
    truncate_string(&redact_text(first.trim()), max_len)
}

fn lane_key_for_snapshot(_view: Option<&str>, _agent_id: Option<&str>) -> String {
    // Meaning-mode: delta baselines must be stable across restarts and must not depend on
    // any lane-like partitioning (agent_id, view, etc).
    "global".to_string()
}

impl McpServer {
    pub(crate) fn tool_tasks_snapshot(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        // Snapshot Navigation Guarantee v1 (line protocol): keep at least one stable navigation
        // handle available even when `max_chars` is so tight that the resume payload degrades to a
        // minimal signal (capsule dropped).
        //
        // We compute the focused target id up-front and stash it in the tool response so the
        // line renderer can still emit `REFERENCE: TASK-*` even under BUDGET_MINIMAL.
        let snapshot_target_id = resolve_target_id(&mut self.store, &workspace, args_obj)
            .ok()
            .map(|(id, _kind, _focus)| id);
        let mut patched = args_obj.clone();

        let delta = match patched.get("delta").and_then(|v| v.as_bool()) {
            Some(value) => value,
            None => {
                if self.dx_mode {
                    patched.insert("delta".to_string(), Value::Bool(true));
                }
                self.dx_mode
            }
        };
        let delta_limit = patched
            .get("delta_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(5);

        // Portal UX: tasks_snapshot is a read-mostly view; default to the relevance-first
        // smart view so the capsule/HUD can safely surface step_focus and multi-agent signals
        // (leases/lanes) without requiring manual view selection.
        if !patched.contains_key("view") {
            patched.insert("view".to_string(), Value::String("smart".to_string()));
        }

        // Portal DX: even in fmt=lines mode, keep snapshot reads deterministically bounded so
        // the portal stays fast (no giant JSON payloads that are later discarded).
        let wants_lines = crate::is_lines_fmt(patched.get("fmt").and_then(|v| v.as_str()));
        if wants_lines
            && !patched.contains_key("context_budget")
            && !patched.contains_key("max_chars")
        {
            let default_budget = match self.toolset {
                crate::Toolset::Core => 6000usize,
                crate::Toolset::Daily => 9000usize,
                crate::Toolset::Full => 12_000usize,
            };
            patched.insert(
                "context_budget".to_string(),
                Value::Number(serde_json::Number::from(default_budget as u64)),
            );
        }

        let relevance_view = patched
            .get("view")
            .and_then(|v| v.as_str())
            .is_some_and(|v| {
                let v = v.trim();
                v.eq_ignore_ascii_case("focus_only")
                    || v.eq_ignore_ascii_case("smart")
                    || v.eq_ignore_ascii_case("explore")
                    || v.eq_ignore_ascii_case("audit")
            })
            || args_obj.contains_key("context_budget");
        if !relevance_view {
            patched
                .entry("graph_diff".to_string())
                .or_insert_with(|| Value::Bool(true));
        }

        let mut response = self.tool_tasks_resume_super(Value::Object(patched));
        if let Some(snapshot_target_id) = snapshot_target_id
            && let Some(obj) = response.as_object_mut()
        {
            obj.insert(
                "snapshot_target_id".to_string(),
                Value::String(snapshot_target_id),
            );
        }
        if let Some(obj) = response.as_object_mut() {
            obj.insert("intent".to_string(), Value::String("snapshot".to_string()));
        }

        // Second-brain core: ensure a mindpack exists (deduped) and surface a stable ref in the
        // capsule so the portal state line can show `pack=mindpack@<seq>` without extra calls.
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !read_only {
            match super::super::admin::mindpack::ensure_mindpack(
                self,
                &workspace,
                true,
                Some("snapshot".to_string()),
                false,
            ) {
                Ok(pack) => {
                    if let Some(pack_ref) = pack.ref_id
                        && let Some(obj) =
                            response.get_mut("result").and_then(|v| v.as_object_mut())
                        && let Some(capsule) =
                            obj.get_mut("capsule").and_then(|v| v.as_object_mut())
                        && let Some(where_obj) =
                            capsule.get_mut("where").and_then(|v| v.as_object_mut())
                    {
                        where_obj.insert("pack".to_string(), json!({ "ref": pack_ref }));
                    }
                }
                Err(err) => {
                    if let Some(obj) = response.as_object_mut() {
                        let entry = warning(
                            "MINDPACK_UNAVAILABLE",
                            "mindpack update unavailable",
                            &format_store_error(err),
                        );
                        match obj.get_mut("warnings") {
                            Some(Value::Array(arr)) => arr.push(entry),
                            Some(_) => {
                                obj.insert("warnings".to_string(), Value::Array(vec![entry]));
                            }
                            None => {
                                obj.insert("warnings".to_string(), Value::Array(vec![entry]));
                            }
                        }
                    }
                }
            }
        }

        // UX: surface the recommended next action (capsule.action) as a portal-first action object.
        // This keeps the "what do I do next?" rail structured and copy/paste-ready without
        // requiring callers to parse the capsule payload.
        let success = response
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if success {
            let (tool, available, purpose, params) = {
                let capsule_action = response
                    .get("result")
                    .and_then(|v| v.get("capsule"))
                    .and_then(|v| v.get("action"))
                    .and_then(|v| v.as_object());

                let tool = capsule_action
                    .and_then(|v| v.get("tool"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let available = capsule_action
                    .and_then(|v| v.get("available"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let purpose = capsule_action
                    .and_then(|v| v.get("purpose"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "next action".to_string());
                let params = capsule_action
                    .and_then(|v| v.get("args").or_else(|| v.get("args_hint")))
                    .cloned()
                    .unwrap_or_else(|| json!({}));

                (tool, available, purpose, params)
            };

            if let Some(tool) = tool
                && available
                && let Some(obj) = response.as_object_mut()
            {
                let entry = suggest_call(&tool, &purpose, "high", params);
                match obj.get_mut("suggestions") {
                    Some(Value::Array(arr)) => arr.push(entry),
                    Some(_) => {
                        obj.insert("suggestions".to_string(), Value::Array(vec![entry]));
                    }
                    None => {
                        obj.insert("suggestions".to_string(), Value::Array(vec![entry]));
                    }
                }
            }
        }

        if delta {
            let view = args_obj.get("view").and_then(|v| v.as_str());
            let include_drafts = view.unwrap_or("").trim().eq_ignore_ascii_case("audit");
            let lane_key = lane_key_for_snapshot(view, None);

            let read_only = args_obj
                .get("read_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let until_seq = match self.store.workspace_last_doc_entry_head(&workspace) {
                Ok(Some(head)) => head.seq,
                Ok(None) => 0,
                Err(err) => {
                    if let Some(obj) = response.as_object_mut()
                        && let Some(arr) = obj.get_mut("warnings").and_then(|v| v.as_array_mut())
                    {
                        arr.push(warning(
                            "DELTA_UNAVAILABLE",
                            "delta mode unavailable (failed to read workspace head)",
                            &format_store_error(err),
                        ));
                    }
                    return response;
                }
            };

            let success = response
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !success {
                return response;
            }

            let Some(result_obj) = response.get("result").and_then(|v| v.as_object()) else {
                return response;
            };
            let Some(target_id) = result_obj
                .get("target")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            else {
                return response;
            };
            let Some(reasoning) = result_obj.get("reasoning_ref").and_then(|v| v.as_object())
            else {
                return response;
            };
            let (Some(branch), Some(notes_doc), Some(graph_doc)) = (
                reasoning
                    .get("branch")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                reasoning
                    .get("notes_doc")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                reasoning
                    .get("graph_doc")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            ) else {
                return response;
            };

            let mut delta_warnings = Vec::<Value>::new();
            let since_seq = match self.store.portal_cursor_get(
                &workspace,
                "tasks_snapshot",
                &target_id,
                &lane_key,
            ) {
                Ok(Some(seq)) => seq,
                Ok(None) => until_seq,
                Err(err) => {
                    delta_warnings.push(warning(
                        "DELTA_UNAVAILABLE",
                        "delta mode unavailable (failed to read stored baseline)",
                        &format_store_error(err),
                    ));
                    if let Some(obj) = response.as_object_mut()
                        && let Some(arr) = obj.get_mut("warnings").and_then(|v| v.as_array_mut())
                    {
                        arr.extend(delta_warnings);
                    }
                    return response;
                }
            };

            // If there's no stored baseline, seed it and return an empty delta so the next call is meaningful.
            if since_seq == until_seq {
                if !read_only {
                    let _ = self.store.portal_cursor_set(
                        &workspace,
                        "tasks_snapshot",
                        &target_id,
                        &lane_key,
                        until_seq.max(0),
                    );
                }
                let delta_value = json!({
                    "mode": "since_last",
                    "since_seq": since_seq,
                    "until_seq": until_seq,
                    "notes": { "count": 0, "items": [], "truncated": false, "dropped": 0 },
                    "cards": { "count": 0, "items": [], "truncated": false, "dropped": 0 },
                    "decisions": { "count": 0, "items": [], "truncated": false, "dropped": 0 },
                    "evidence": { "count": 0, "items": [], "truncated": false, "dropped": 0 }
                });
                if let Some(result_obj) = response.get_mut("result").and_then(|v| v.as_object_mut())
                {
                    result_obj.insert("delta".to_string(), delta_value);
                }
                return response;
            }

            let scan_limit = delta_limit.saturating_mul(20).max(delta_limit + 5).min(200);

            let notes_res = self.store.doc_entries_since(
                &workspace,
                bm_storage::DocEntriesSinceRequest {
                    branch: branch.clone(),
                    doc: notes_doc.clone(),
                    since_seq,
                    limit: scan_limit,
                    kind: Some(bm_storage::DocEntryKind::Note),
                },
            );
            let mut note_items = Vec::<Value>::new();
            let mut note_truncated = false;
            let mut note_dropped = 0usize;
            if let Ok(res) = notes_res {
                let mut visible = Vec::<bm_storage::DocEntryRow>::new();
                for e in res.entries {
                    if e.seq > until_seq {
                        continue;
                    }
                    let meta_val = e
                        .meta_json
                        .as_ref()
                        .map(|raw| parse_json_or_string(raw))
                        .unwrap_or(Value::Null);
                    if !include_drafts && meta_is_draft(&meta_val) {
                        continue;
                    }
                    visible.push(e);
                }
                note_truncated = visible.len() > delta_limit || res.total > scan_limit;
                note_dropped = visible.len().saturating_sub(delta_limit);
                for e in visible.into_iter().take(delta_limit) {
                    note_items.push(json!({
                        "ref": format!("{}@{}", e.doc, e.seq),
                        "seq": e.seq,
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "title": e.title,
                        "summary": delta_summary_one_line(e.title.as_deref(), e.content.as_deref(), 140)
                    }));
                }
            } else if let Err(err) = notes_res {
                delta_warnings.push(warning(
                    "DELTA_UNAVAILABLE",
                    "delta notes unavailable",
                    &format_store_error(err),
                ));
            }

            let cards_res = self
                .store
                .graph_cards_since(&workspace, &branch, &graph_doc, since_seq, scan_limit);
            let mut cards_visible = Vec::<bm_storage::GraphNodeRow>::new();
            let mut decisions_visible = Vec::<bm_storage::GraphNodeRow>::new();
            let mut evidence_visible = Vec::<bm_storage::GraphNodeRow>::new();
            if let Ok((nodes, _total)) = cards_res {
                let mut visible = Vec::<bm_storage::GraphNodeRow>::new();
                for n in nodes {
                    if n.last_seq > until_seq {
                        continue;
                    }
                    if !tags_visibility_allows(&n.tags, include_drafts, None) {
                        continue;
                    }
                    visible.push(n);
                }
                for n in visible {
                    match n.node_type.as_str() {
                        "decision" => decisions_visible.push(n),
                        "evidence" => evidence_visible.push(n),
                        _ => cards_visible.push(n),
                    }
                }
            } else if let Err(err) = cards_res {
                delta_warnings.push(warning(
                    "DELTA_UNAVAILABLE",
                    "delta cards unavailable",
                    &format_store_error(err),
                ));
            }

            let cards_truncated = cards_visible.len() > delta_limit;
            let cards_dropped = cards_visible.len().saturating_sub(delta_limit);
            let decisions_truncated = decisions_visible.len() > delta_limit;
            let decisions_dropped = decisions_visible.len().saturating_sub(delta_limit);
            let evidence_truncated = evidence_visible.len() > delta_limit;
            let evidence_dropped = evidence_visible.len().saturating_sub(delta_limit);

            let cards = cards_visible
                .into_iter()
                .take(delta_limit)
                .map(|n| {
                    json!({
                        "id": n.id,
                        "type": n.node_type,
                        "seq": n.last_seq,
                        "title": n.title,
                        "summary": delta_summary_one_line(n.title.as_deref(), n.text.as_deref(), 140)
                    })
                })
                .collect::<Vec<_>>();
            let decisions = decisions_visible
                .into_iter()
                .take(delta_limit)
                .map(|n| {
                    json!({
                        "id": n.id,
                        "type": n.node_type,
                        "seq": n.last_seq,
                        "title": n.title,
                        "summary": delta_summary_one_line(n.title.as_deref(), n.text.as_deref(), 140)
                    })
                })
                .collect::<Vec<_>>();
            let evidence = evidence_visible
                .into_iter()
                .take(delta_limit)
                .map(|n| {
                    json!({
                        "id": n.id,
                        "type": n.node_type,
                        "seq": n.last_seq,
                        "title": n.title,
                        "summary": delta_summary_one_line(n.title.as_deref(), n.text.as_deref(), 140)
                    })
                })
                .collect::<Vec<_>>();

            let delta_value = json!({
                "mode": "since_last",
                "since_seq": since_seq,
                "until_seq": until_seq,
                "notes": { "count": note_items.len(), "items": note_items, "truncated": note_truncated, "dropped": note_dropped },
                "cards": { "count": cards.len(), "items": cards, "truncated": cards_truncated, "dropped": cards_dropped },
                "decisions": { "count": decisions.len(), "items": decisions, "truncated": decisions_truncated, "dropped": decisions_dropped },
                "evidence": { "count": evidence.len(), "items": evidence, "truncated": evidence_truncated, "dropped": evidence_dropped }
            });

            if let Some(result_obj) = response.get_mut("result").and_then(|v| v.as_object_mut()) {
                result_obj.insert("delta".to_string(), delta_value);
            }

            if !delta_warnings.is_empty()
                && let Some(obj) = response.as_object_mut()
                && let Some(arr) = obj.get_mut("warnings").and_then(|v| v.as_array_mut())
            {
                arr.extend(delta_warnings);
            }

            if !read_only {
                let _ = self.store.portal_cursor_set(
                    &workspace,
                    "tasks_snapshot",
                    &target_id,
                    &lane_key,
                    until_seq.max(0),
                );
            }
        }
        response
    }

    pub(crate) fn tool_tasks_context_pack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let delta_limit = args_obj
            .get("delta_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let context = match build_radar_context_with_options(
            &mut self.store,
            &workspace,
            &target_id,
            kind,
            read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut events = if delta_limit == 0 {
            Vec::new()
        } else {
            match self
                .store
                .list_events_for_task(&workspace, &target_id, delta_limit)
            {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };
        events.reverse();
        sort_events_by_seq(&mut events);
        let events_total = events.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "target": context.target,
            "radar": context.radar,
            "delta": {
                "limit": delta_limit,
                "events": events_to_json(events)
            }
        });
        if let Some(steps) = context.steps
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("steps".to_string(), steps);
        }

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |= compact_event_payloads_at(&mut result, &["delta", "events"]);
            }
            truncated |= trim_array_to_budget(&mut result, &["delta", "events"], limit, true);
            let events_empty = result
                .get("delta")
                .and_then(|v| v.get("events"))
                .and_then(|v| v.as_array())
                .map(|events| events.is_empty())
                .unwrap_or(true);
            if events_empty
                && events_total > 0
                && ensure_minimal_list_at(&mut result, &["delta", "events"], events_total, "events")
            {
                truncated = true;
                minimal = true;
            }
            if json_len_chars(&result) > limit {
                let mut removed_any = false;
                if let Some(first) = result
                    .get_mut("steps")
                    .and_then(|v| v.as_object_mut())
                    .and_then(|steps| steps.get_mut("first_open"))
                    .and_then(|v| v.as_object_mut())
                {
                    for key in [
                        "criteria_confirmed",
                        "tests_confirmed",
                        "security_confirmed",
                        "perf_confirmed",
                        "docs_confirmed",
                    ] {
                        removed_any |= first.remove(key).is_some();
                    }
                }
                truncated |= removed_any;
            }
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["steps"], limit, false);
            }
            let (_used, trimmed_fields) = enforce_max_chars_budget(&mut result, limit);
            truncated |= trimmed_fields;
            if json_len_chars(&result) > limit {
                if compact_radar_for_budget(&mut result) {
                    truncated = true;
                }
                if compact_target_for_budget(&mut result) {
                    truncated = true;
                }
            }
            if json_len_chars(&result) > limit {
                let removed = result
                    .get_mut("radar")
                    .and_then(|v| v.as_object_mut())
                    .map(|radar| radar.remove("why").is_some())
                    .unwrap_or(false);
                truncated |= removed;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    changed |= compact_event_payloads_at(value, &["delta", "events"]);
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["delta", "events"], true);
                    }
                    if json_len_chars(value) > limit {
                        changed |= ensure_minimal_list_at(
                            value,
                            &["delta", "events"],
                            events_total,
                            "events",
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["steps"], &["first_open"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["steps"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= compact_radar_for_budget(value);
                        changed |= compact_target_for_budget(value);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(
                            value,
                            &["radar"],
                            &["why", "verify", "next", "blockers"],
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["delta"], &["events"]);
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("context_pack", result)
        } else {
            ai_ok_with_warnings("context_pack", result, warnings, Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let dir = std::env::temp_dir().join(format!("bm_snapshot_dx_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn build_server(dx_mode: bool) -> (McpServer, PathBuf) {
        let dir = temp_dir();
        let store = SqliteStore::open(&dir).expect("open store");
        let runner_autostart_enabled =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let runner_autostart_state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
        (
            McpServer::new(
                store,
                crate::McpServerConfig {
                    toolset: crate::Toolset::Daily,
                    response_verbosity: crate::ResponseVerbosity::Full,
                    dx_mode,
                    ux_proof_v2_enabled: true,
                    knowledge_autolint_enabled: true,
                    note_promote_enabled: true,
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
            ),
            dir,
        )
    }

    #[test]
    fn snapshot_defaults_delta_in_dx_mode() {
        let (mut server, dir) = build_server(true);
        let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
        server.store.workspace_init(&workspace).unwrap();

        let plan_payload_json = json!({
            "kind": "plan",
            "title": "DX Snapshot Plan",
            "parent": null
        })
        .to_string();
        let (plan_id, _plan_revision, _plan_event) = server
            .store
            .create(
                &workspace,
                bm_storage::TaskCreateRequest {
                    kind: crate::TaskKind::Plan,
                    title: "DX Snapshot Plan".to_string(),
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: plan_payload_json,
                },
            )
            .expect("create plan");

        let task_payload_json = json!({
            "kind": "task",
            "title": "DX Snapshot Task",
            "parent": plan_id
        })
        .to_string();
        let (task_id, _revision, _event) = server
            .store
            .create(
                &workspace,
                bm_storage::TaskCreateRequest {
                    kind: crate::TaskKind::Task,
                    title: "DX Snapshot Task".to_string(),
                    parent_plan_id: Some(plan_id),
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "task_created".to_string(),
                    event_payload_json: task_payload_json,
                },
            )
            .expect("create task");

        let resp = server.tool_tasks_snapshot(json!({ "workspace": "demo", "task": task_id }));
        assert_eq!(
            resp.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "snapshot should succeed"
        );
        let result = resp.get("result").expect("result");
        assert!(
            result.get("delta").is_some(),
            "dx mode should default delta"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
