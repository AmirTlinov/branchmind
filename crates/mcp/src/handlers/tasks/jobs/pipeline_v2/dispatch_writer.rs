#![forbid(unsafe_code)]

use super::super::*;
use super::{DEFAULT_EXECUTOR_PROFILE, DEFAULT_JOBS_MODEL};
use super::{
    MeshMessageRequest, optional_non_empty_string, parse_meta_map, publish_optional_mesh_message,
    require_non_empty_string, scout_policy_from_meta,
};
use crate::support::{
    ensure_artifact_ref, extract_job_id_from_ref, parse_json_object_from_text,
    validate_scout_context_pack as validate_scout_context_pack_contract,
};
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_jobs_macro_dispatch_writer(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "slice_id",
                "scout_pack_ref",
                "objective",
                "dod",
                "executor",
                "executor_profile",
                "model",
                "dry_run",
                "idempotency_key",
                "from_agent_id",
                "thread_id",
                "meta",
            ],
            "jobs.macro.dispatch.writer",
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
        let scout_pack_ref = match args_obj.get("scout_pack_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "scout_pack_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "scout_pack_ref is required"),
        };
        let scout_job_id = match extract_job_id_from_ref(&scout_pack_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "scout_pack_ref must include a JOB-... lineage token",
                );
            }
        };
        let objective = match require_non_empty_string(args_obj, "objective") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let dod = match args_obj.get("dod") {
            Some(v) if v.is_object() => v.clone(),
            Some(_) => return ai_error("INVALID_INPUT", "dod must be an object"),
            None => return ai_error("INVALID_INPUT", "dod is required"),
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let idempotency_key = match optional_non_empty_string(args_obj, "idempotency_key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_agent_id = match optional_agent_id(args_obj, "from_agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let thread_id = match optional_non_empty_string(args_obj, "thread_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let executor = match optional_non_empty_string(args_obj, "executor") {
            Ok(v) => v.unwrap_or_else(|| "codex".to_string()),
            Err(resp) => return resp,
        };
        if !matches!(executor.as_str(), "codex" | "claude_code" | "auto") {
            return ai_error("INVALID_INPUT", "executor must be codex|claude_code|auto");
        }
        let executor_profile = match optional_non_empty_string(args_obj, "executor_profile") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_EXECUTOR_PROFILE.to_string()),
            Err(resp) => return resp,
        };
        let model = match optional_non_empty_string(args_obj, "model") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_JOBS_MODEL.to_string()),
            Err(resp) => return resp,
        };
        if !model.eq_ignore_ascii_case(DEFAULT_JOBS_MODEL) {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.writer: model must be gpt-5.3-codex",
            );
        }

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        // Validate scout job is DONE and has valid pack.
        let scout_open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: scout_job_id.clone(),
                include_prompt: false,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => {
                return ai_error("UNKNOWN_ID", "Unknown scout job id from scout_pack_ref");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !scout_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.macro.dispatch.writer: scout job is not DONE",
            );
        }
        let scout_summary = match scout_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.macro.dispatch.writer: scout job summary is empty",
                );
            }
        };
        let scout_json = match parse_json_object_from_text(
            scout_summary,
            "scout summary (scout_context_pack)",
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let scout_meta = parse_meta_map(scout_open.meta_json.as_deref());
        let scout_max_context_refs = scout_meta
            .get("max_context_refs")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(24)
            .clamp(8, 64);
        let scout_policy = scout_policy_from_meta(&scout_meta);
        let (_, scout_contract_warnings) = match validate_scout_context_pack_contract(
            &self.store,
            &workspace,
            &scout_json,
            scout_max_context_refs,
            &scout_policy,
        ) {
            Ok(v) => v,
            Err(_) => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.macro.dispatch.writer: scout pack failed strict quality contract",
                );
            }
        };
        warnings.extend(scout_contract_warnings);

        // Try to read rendered scout artifact for writer prompt injection.
        let scout_rendered = self
            .store
            .job_artifact_get(
                &workspace,
                bm_storage::JobArtifactGetRequest {
                    job_id: scout_job_id.clone(),
                    artifact_key: "scout_context_rendered".to_string(),
                },
            )
            .ok()
            .flatten()
            .map(|a| a.content_text);

        // Build writer job.
        let mut meta = args_obj
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        meta.insert("role".to_string(), Value::String("writer".to_string()));
        meta.insert(
            "pipeline_role".to_string(),
            Value::String("writer".to_string()),
        );
        meta.insert(
            "dispatched_by".to_string(),
            Value::String("jobs.macro.dispatch.writer".to_string()),
        );
        meta.insert("task".to_string(), Value::String(task_id.clone()));
        meta.insert("slice_id".to_string(), Value::String(slice_id.clone()));
        meta.insert(
            "scout_pack_ref".to_string(),
            Value::String(scout_pack_ref.clone()),
        );
        meta.insert(
            "scout_job_id".to_string(),
            Value::String(scout_job_id.clone()),
        );
        meta.insert("objective".to_string(), Value::String(objective.clone()));
        meta.insert("dod".to_string(), dod.clone());
        meta.insert("executor".to_string(), Value::String(executor.clone()));
        meta.insert(
            "executor_profile".to_string(),
            Value::String(executor_profile.clone()),
        );
        meta.insert("executor_model".to_string(), Value::String(model.clone()));
        meta.insert(
            "expected_artifacts".to_string(),
            Value::Array(vec![Value::String("writer_patch_pack".to_string())]),
        );
        meta.insert(
            "pipeline".to_string(),
            json!({
                "task": task_id,
                "slice_id": slice_id,
                "scout_pack_ref": scout_pack_ref
            }),
        );
        let meta_json = serde_json::to_string(&Value::Object(meta)).ok();
        let title = format!("Writer patches for {slice_id}");
        let priority = "MEDIUM".to_string();
        let dod_text = serde_json::to_string_pretty(&dod)
            .or_else(|_| serde_json::to_string(&dod))
            .unwrap_or_else(|_| "{}".to_string());

        // Build writer prompt with injected scout context.
        let scout_context_block = scout_rendered
            .as_deref()
            .unwrap_or("[scout context artifact not available — use scout_pack_ref]");
        let role_prompt = format!(
            "ROLE=WRITER\n\
MUST output ONLY writer_patch_pack JSON in summary field.\n\
MUST NOT write files to disk. MUST NOT run shell commands.\n\
PatchOp kinds: replace, insert_after, insert_before, create_file, delete_file.\n\
If scout context is insufficient, set insufficient_context and leave patches empty.\n\n\
Task: {task_id}\nSlice: {slice_id}\nObjective: {objective}\nScout pack ref: {scout_pack_ref}\n\n\
DoD:\n{dod_text}\n\n\
--- SCOUT CONTEXT (inline) ---\n{scout_context_block}\n--- END SCOUT CONTEXT ---\n"
        );

        let created = if dry_run {
            None
        } else {
            match self.store.job_create(
                &workspace,
                bm_storage::JobCreateRequest {
                    title: title.clone(),
                    prompt: role_prompt,
                    kind: "codex_cli".to_string(),
                    priority,
                    task_id: Some(task_id.clone()),
                    anchor_id: None,
                    meta_json,
                },
            ) {
                Ok(v) => Some(v),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };

        let mesh = if dry_run {
            None
        } else {
            match publish_optional_mesh_message(
                self,
                &workspace,
                MeshMessageRequest {
                    task_id: Some(task_id.clone()),
                    from_agent_id,
                    thread_id,
                    idempotency_key,
                    kind: "dispatch.writer".to_string(),
                    summary: format!("writer dispatched: {title}"),
                    payload: json!({
                    "role": "writer",
                    "task": task_id,
                    "slice_id": slice_id,
                    "scout_pack_ref": scout_pack_ref
                    }),
                },
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "dry_run": dry_run,
            "job": created.as_ref().map(|v| job_row_to_json(v.job.clone())).unwrap_or(Value::Null),
            "event": created.as_ref().map(|v| job_event_to_json(v.created_event.clone())).unwrap_or(Value::Null),
            "routing": {
                "role": "writer",
                "executor": executor,
                "executor_profile": executor_profile,
                "executor_model": model,
                "expected_artifacts": ["writer_patch_pack"]
            },
            "scout_context_injected": scout_rendered.is_some(),
            "mesh": mesh
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_dispatch_writer", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_dispatch_writer",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
