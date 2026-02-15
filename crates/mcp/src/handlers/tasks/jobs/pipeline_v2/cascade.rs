#![forbid(unsafe_code)]

use super::super::*;
use super::require_non_empty_string;
use crate::support::CascadeSession;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_cascade_init(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "anchor",
                "slice_id",
                "objective",
                "constraints",
                "max_context_refs",
                "quality_profile",
                "novelty_policy",
                "dry_run",
                "meta",
            ],
            "jobs.pipeline.cascade.init",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task_id = match require_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match require_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        // Create cascade session and embed it in meta.
        let session_id = format!("pls-{:08x}", crate::support::now_ms_i64() as u32);
        let session = CascadeSession::new(session_id.clone());

        let mut scout_args = args_obj.clone();
        // Inject cascade session into meta.
        let mut meta = scout_args
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        meta.insert("pipeline_session".to_string(), session.to_json());
        scout_args.insert("meta".to_string(), Value::Object(meta));

        // Dispatch scout as the first pipeline stage.
        let scout_result = self.tool_tasks_jobs_macro_dispatch_scout(Value::Object(scout_args));

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let result = json!({
            "workspace": workspace.as_str(),
            "cascade_session_id": session_id,
            "phase": "scout",
            "task": task_id,
            "slice_id": slice_id,
            "dry_run": dry_run,
            "scout_dispatch": scout_result
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_cascade_init", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_pipeline_cascade_init",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    // ── cascade.advance ──

    pub(crate) fn tool_tasks_jobs_pipeline_cascade_advance(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &["workspace", "session_json", "event", "hints", "job_id"],
            "jobs.pipeline.cascade.advance",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let event = match require_non_empty_string(args_obj, "event") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let session_json = match args_obj.get("session_json") {
            Some(v) => v.clone(),
            None => return ai_error("INVALID_INPUT", "session_json is required"),
        };
        let mut session = match CascadeSession::from_json(&session_json) {
            Some(s) => s,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "session_json: failed to parse cascade session",
                );
            }
        };

        let hints = args_obj
            .get("hints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Track completed job in lineage.
        if let Some(job_id) = args_obj
            .get("job_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            match session.phase.as_str() {
                "scout" | "pre_validate" => session.scout_job_ids.push(job_id),
                "writer" => session.writer_job_ids.push(job_id),
                "post_validate" => session.validator_job_ids.push(job_id),
                _ => {}
            }
        }

        let action = crate::support::cascade_advance(&mut session, &event, hints);

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let result = json!({
            "workspace": workspace.as_str(),
            "session": session.to_json(),
            "action": format!("{action:?}"),
            "phase": session.phase.as_str(),
            "escalated": session.phase.as_str() == "escalated"
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_cascade_advance", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_pipeline_cascade_advance",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
