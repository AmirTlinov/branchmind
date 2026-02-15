#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_open(server: &mut McpServer, args: Value) -> Value {
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
            "job",
            "include_artifacts",
            "include_prompt",
            "include_events",
            "include_meta",
            "max_events",
            "before_seq",
            "max_chars",
            "fmt",
        ],
        "jobs.open",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let job_id = match require_string(args_obj, "job") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let include_prompt = match optional_bool(args_obj, "include_prompt") {
        Ok(v) => v.unwrap_or(false),
        Err(resp) => return resp,
    };
    let include_events = match optional_bool(args_obj, "include_events") {
        Ok(v) => v.unwrap_or(true),
        Err(resp) => return resp,
    };
    let include_meta = match optional_bool(args_obj, "include_meta") {
        Ok(v) => v.unwrap_or(false),
        Err(resp) => return resp,
    };
    let include_artifacts = match optional_bool(args_obj, "include_artifacts") {
        Ok(v) => v.unwrap_or(false),
        Err(resp) => return resp,
    };
    let max_events = match optional_usize(args_obj, "max_events") {
        Ok(v) => v.unwrap_or(10).clamp(0, 200),
        Err(resp) => return resp,
    };
    let before_seq = match optional_i64(args_obj, "before_seq") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let before_seq = match before_seq {
        Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "before_seq must be > 0"),
        Some(v) => Some(v),
        None => None,
    };
    let max_chars = match optional_usize(args_obj, "max_chars") {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let open = match server.store.job_open(
        &workspace,
        bm_storage::JobOpenRequest {
            id: job_id,
            include_prompt,
            include_events,
            // include_artifacts UX needs meta (expected_artifacts) even if the caller doesn't
            // request the full meta payload in the response.
            include_meta: include_meta || include_artifacts,
            max_events,
            before_seq,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown job id",
                Some(
                    "Call jobs op=radar (or jobs op=list) to pick a valid job id, then retry jobs op=open.",
                ),
                vec![suggest_call(
                    "jobs",
                    "List current jobs before retrying jobs.open",
                    "high",
                    json!({
                        "op": "radar",
                        "args": { "workspace": workspace.as_str(), "limit": 20 }
                    }),
                )],
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let job_id_for_ref = open.job.id.clone();
    let events = open
        .events
        .into_iter()
        .map(job_event_to_json)
        .collect::<Vec<_>>();
    let mut result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(open.job),
        "prompt": open.prompt,
        "events": events,
        "count": events.len(),
        "has_more_events": open.has_more_events,
        "truncated": false
    });

    if include_meta {
        let meta_value = open
            .meta_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        if let Some(obj) = result.as_object_mut() {
            obj.insert("meta".to_string(), meta_value);
        }
    }

    if include_artifacts {
        let expected = crate::support::expected_artifacts_from_meta_json(open.meta_json.as_deref());
        let artifacts = match server.store.job_artifacts_list(
            &workspace,
            bm_storage::JobArtifactsListRequest {
                job_id: job_id_for_ref.clone(),
                limit: 8,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut stored_by_key =
            std::collections::BTreeMap::<String, bm_storage::JobArtifactMetaRow>::new();
        for row in artifacts {
            stored_by_key.insert(row.artifact_key.clone(), row);
        }

        let mut out = Vec::<Value>::new();
        let mut actions = Vec::<Value>::new();
        let mut seen = std::collections::BTreeSet::<String>::new();

        // 1) expected (including missing)
        for key in expected {
            let artifact_ref = format!("artifact://jobs/{}/{}", job_id_for_ref, key);
            if let Some(stored) = stored_by_key.get(&key) {
                out.push(json!({
                    "artifact_key": key,
                    "status": "stored",
                    "content_len": stored.content_len,
                    "created_at_ms": stored.created_at_ms,
                    "artifact_ref": artifact_ref
                }));
            } else {
                out.push(json!({
                    "artifact_key": key,
                    "status": "missing",
                    "content_len": Value::Null,
                    "created_at_ms": Value::Null,
                    "artifact_ref": artifact_ref
                }));
            }
            seen.insert(key.clone());

            actions.push(json!({
                "tool": "open",
                "reason": "Open this job artifact by stable ref (read-only).",
                "priority": "high",
                "args": {
                    "workspace": workspace.as_str(),
                    "id": artifact_ref,
                    "max_chars": 4000
                }
            }));
            actions.push(json!({
                "tool": "jobs",
                "reason": "Read a bounded slice of this artifact (paged via offset).",
                "priority": "high",
                "op": "call",
                "cmd": "jobs.artifact.get",
                "args": {
                    "workspace": workspace.as_str(),
                    "job": job_id_for_ref.clone(),
                    "artifact_key": key,
                    "offset": 0,
                    "max_chars": 4000
                }
            }));
        }

        // 2) stored-but-unexpected (still show; deterministic order)
        for (key, stored) in stored_by_key {
            if seen.contains(&key) {
                continue;
            }
            let artifact_ref = format!("artifact://jobs/{}/{}", job_id_for_ref, key);
            out.push(json!({
                "artifact_key": key,
                "status": "stored",
                "content_len": stored.content_len,
                "created_at_ms": stored.created_at_ms,
                "artifact_ref": artifact_ref
            }));

            actions.push(json!({
                "tool": "open",
                "reason": "Open this job artifact by stable ref (read-only).",
                "priority": "high",
                "args": {
                    "workspace": workspace.as_str(),
                    "id": artifact_ref,
                    "max_chars": 4000
                }
            }));
            actions.push(json!({
                "tool": "jobs",
                "reason": "Read a bounded slice of this artifact (paged via offset).",
                "priority": "high",
                "op": "call",
                "cmd": "jobs.artifact.get",
                "args": {
                    "workspace": workspace.as_str(),
                    "job": job_id_for_ref.clone(),
                    "artifact_key": key,
                    "offset": 0,
                    "max_chars": 4000
                }
            }));
        }

        if let Some(obj) = result.as_object_mut() {
            obj.insert("artifacts".to_string(), Value::Array(out));
            obj.insert("actions".to_string(), Value::Array(actions));
        }
    }

    if let Some(limit) = max_chars {
        let (limit, clamped) = clamp_budget_max(limit);
        let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "events", limit);
        let mut truncated = budget_truncated;

        // If we're still over budget, drop the prompt as a last resort.
        if json_len_chars(&result) > limit {
            if let Some(obj) = result.as_object_mut()
                && obj.remove("prompt").is_some()
            {
                truncated = true;
            }
            let (_used2, events_truncated2) =
                enforce_graph_list_budget(&mut result, "events", limit);
            truncated = truncated || events_truncated2;
        }

        if let Some(obj) = result.as_object_mut()
            && let Some(events) = obj.get("events").and_then(|v| v.as_array())
        {
            obj.insert(
                "count".to_string(),
                Value::Number(serde_json::Number::from(events.len() as u64)),
            );
            let has_more = obj
                .get("has_more_events")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if truncated && !has_more {
                obj.insert("has_more_events".to_string(), Value::Bool(true));
            }
        }

        set_truncated_flag(&mut result, truncated);
        let _used = attach_budget(&mut result, limit, truncated);

        let mut warnings = budget_warnings(truncated, false, clamped);
        push_warning_if(&mut warnings, unknown_warning);
        if warnings.is_empty() {
            ai_ok("tasks_jobs_open", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_open", result, warnings, Vec::new())
        }
    } else if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_open", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_open", result)
    }
}
