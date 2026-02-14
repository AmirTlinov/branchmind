#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_jobs_macro_dispatch_validator(&mut self, args: Value) -> Value {
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
                "builder_batch_ref",
                "plan_ref",
                "executor",
                "executor_profile",
                "model",
                "dry_run",
                "idempotency_key",
                "from_agent_id",
                "thread_id",
                "meta",
            ],
            "jobs.macro.dispatch.validator",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match require_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let binding = match resolve_slice_binding_optional(self, &workspace, &slice_id) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if binding.is_none() && self.jobs_slice_first_fail_closed_enabled {
            return ai_error(
                "PRECONDITION_FAILED",
                "unknown slice_id: missing plan_slices binding (run tasks.slices.apply first)",
            );
        }
        let slice_first = binding.is_some();

        let task_arg = match optional_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task_id = match (task_arg, binding.as_ref()) {
            (Some(v), _) => v,
            (None, Some(binding)) => binding.plan_id.clone(),
            (None, None) => {
                return ai_error(
                    "INVALID_INPUT",
                    "task is required when slice binding is missing (legacy/unplanned mode)",
                );
            }
        };
        if let Some(binding) = binding.as_ref()
            && task_id != binding.plan_id
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.macro.dispatch.validator: task must match slice binding plan_id (slice-first)",
            );
        }
        let scout_pack_ref = match args_obj.get("scout_pack_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "scout_pack_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "scout_pack_ref is required"),
        };
        let builder_batch_ref = match args_obj.get("builder_batch_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "builder_batch_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "builder_batch_ref is required"),
        };
        let plan_ref = match optional_non_empty_string(args_obj, "plan_ref") {
            Ok(v) => v.unwrap_or_else(|| {
                binding
                    .as_ref()
                    .map(|b| b.plan_id.clone())
                    .unwrap_or_else(|| task_id.clone())
            }),
            Err(resp) => return resp,
        };
        let builder_job_id = match extract_job_id_from_ref(&builder_batch_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "builder_batch_ref must include a JOB-... lineage token",
                );
            }
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
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        // Legacy/unplanned mode: best-effort recover budgets/objective from builder meta to keep
        // validator prompts deterministic and aligned with gate/apply enforcement.
        let mut legacy_objective: Option<String> = None;
        let mut legacy_budgets_json: Option<Value> = None;
        if !slice_first && !dry_run {
            let builder_open = match self.store.job_open(
                &workspace,
                bm_storage::JobOpenRequest {
                    id: builder_job_id.clone(),
                    include_prompt: false,
                    include_events: false,
                    include_meta: true,
                    max_events: 0,
                    before_seq: None,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownId) => {
                    return ai_error(
                        "UNKNOWN_ID",
                        "Unknown builder job id from builder_batch_ref",
                    );
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let meta_map = parse_meta_map(builder_open.meta_json.as_deref());
            legacy_objective = meta_map
                .get("objective")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            legacy_budgets_json = meta_map
                .get("slice_budgets")
                .cloned()
                .filter(|v| v.is_object());
        }
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
            Ok(v) => v
                .unwrap_or_else(|| DEFAULT_VALIDATOR_EXECUTOR.to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if executor != DEFAULT_VALIDATOR_EXECUTOR {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.validator: executor must be claude_code",
            );
        }
        let executor_profile = match optional_non_empty_string(args_obj, "executor_profile") {
            Ok(v) => v
                .unwrap_or_else(|| DEFAULT_VALIDATOR_PROFILE.to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if executor_profile != DEFAULT_VALIDATOR_PROFILE {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.validator: executor_profile must be audit",
            );
        }
        let model = match optional_non_empty_string(args_obj, "model") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_VALIDATOR_MODEL.to_string()),
            Err(resp) => return resp,
        };
        let model_lc = model.to_ascii_lowercase();
        if !model_lc.contains("opus") || !model_lc.contains("4.6") {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.validator: model must be opus-4.6 family",
            );
        }

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let mut meta = args_obj
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if meta
            .get("parent_job")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == builder_job_id)
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.macro.dispatch.validator: validator must be lineage-independent from builder",
            );
        }
        meta.insert("role".to_string(), Value::String("validator".to_string()));
        meta.insert(
            "pipeline_role".to_string(),
            Value::String("validator".to_string()),
        );
        meta.insert(
            "dispatched_by".to_string(),
            Value::String("jobs.macro.dispatch.validator".to_string()),
        );
        meta.insert("task".to_string(), Value::String(task_id.clone()));
        meta.insert("slice_id".to_string(), Value::String(slice_id.clone()));
        if let Some(binding) = binding.as_ref() {
            meta.insert(
                "slice_task_id".to_string(),
                Value::String(binding.slice_task_id.clone()),
            );
            meta.insert("slice_budgets".to_string(), binding.spec.budgets.to_json());
        } else {
            meta.insert(
                "slice_budgets".to_string(),
                legacy_budgets_json
                    .clone()
                    .unwrap_or_else(|| crate::support::SliceBudgets::default().to_json()),
            );
        }
        meta.insert("plan_id".to_string(), Value::String(task_id.clone()));
        meta.insert("plan_ref".to_string(), Value::String(plan_ref.clone()));
        meta.insert(
            "scout_pack_ref".to_string(),
            Value::String(scout_pack_ref.clone()),
        );
        meta.insert(
            "builder_batch_ref".to_string(),
            Value::String(builder_batch_ref.clone()),
        );
        meta.insert(
            "builder_job_id".to_string(),
            Value::String(builder_job_id.clone()),
        );
        meta.insert("scout_job_id".to_string(), Value::String(scout_job_id));
        meta.insert("executor".to_string(), Value::String(executor.clone()));
        meta.insert(
            "executor_profile".to_string(),
            Value::String(executor_profile.clone()),
        );
        meta.insert("executor_model".to_string(), Value::String(model.clone()));
        meta.insert(
            "expected_artifacts".to_string(),
            Value::Array(vec![Value::String("validator_report".to_string())]),
        );
        let mut pipeline = json!({
            "task": task_id,
            "slice_id": slice_id,
            "scout_pack_ref": scout_pack_ref,
            "builder_batch_ref": builder_batch_ref,
            "plan_ref": plan_ref
        });
        if slice_first
            && let Some(binding) = binding.as_ref()
            && let Some(obj) = pipeline.as_object_mut()
        {
            obj.insert(
                "slice_task_id".to_string(),
                Value::String(binding.slice_task_id.clone()),
            );
        }
        meta.insert("pipeline".to_string(), pipeline);
        let meta_json = serde_json::to_string(&Value::Object(meta)).ok();
        let title = format!("Validator review for {slice_id}");
        let priority = "MEDIUM".to_string();

        let slice_task_id = binding
            .as_ref()
            .map(|b| b.slice_task_id.clone())
            .unwrap_or_else(|| "-".to_string());
        let budgets_json = binding
            .as_ref()
            .map(|b| b.spec.budgets.to_json())
            .or_else(|| legacy_budgets_json.clone())
            .unwrap_or_else(|| crate::support::SliceBudgets::default().to_json());
        let slice_spec_json = if let Some(binding) = binding.as_ref() {
            binding.spec.to_json()
        } else {
            let objective = legacy_objective
                .clone()
                .unwrap_or_else(|| format!("Slice {slice_id}"));
            let mut spec = crate::support::propose_next_slice_spec(&task_id, "", &objective, &[]);
            if let Some(obj) = budgets_json.as_object() {
                if let Some(v) = obj.get("max_context_refs").and_then(|v| v.as_u64()) {
                    spec.budgets.max_context_refs = (v as usize).clamp(8, 64);
                }
                if let Some(v) = obj.get("max_files").and_then(|v| v.as_u64()) {
                    spec.budgets.max_files = (v as usize).clamp(1, 200);
                }
                if let Some(v) = obj.get("max_diff_lines").and_then(|v| v.as_u64()) {
                    spec.budgets.max_diff_lines = (v as usize).clamp(1, 200_000);
                }
            }
            spec.to_json()
        };
        let slice_plan_spec_text = serde_json::to_string_pretty(&slice_spec_json)
            .or_else(|_| serde_json::to_string(&slice_spec_json))
            .unwrap_or_else(|_| "{}".to_string());
        let budgets_text = serde_json::to_string_pretty(&budgets_json)
            .or_else(|_| serde_json::to_string(&budgets_json))
            .unwrap_or_else(|_| "{}".to_string());
        let role_prompt = format!(
            "ROLE=VALIDATOR\n\
MUST perform independent verification.\n\
MUST output ONLY validator_report JSON.\n\
MUST include plan-fit score and concrete rework actions.\n\
MUST reject when execution_evidence is missing or ambiguous.\n\
Execution target: executor={executor} model={model} profile={executor_profile}.\n\n\
Plan: {task_id}\nSlice: {slice_id}\nSlice task: {slice_task_id}\nPlan ref: {plan_ref}\n\n\
Budgets (fail-closed):\n{budgets_text}\n\n\
SlicePlanSpec (source of truth, bounded):\n{slice_plan_spec_text}\n\n\
Scout pack ref: {scout_pack_ref}\nBuilder batch ref: {builder_batch_ref}\n"
        );
        let job_kind = "claude_cli";

        let created = if dry_run {
            None
        } else {
            match self.store.job_create(
                &workspace,
                bm_storage::JobCreateRequest {
                    title: title.clone(),
                    prompt: role_prompt,
                    kind: job_kind.to_string(),
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
                    kind: "dispatch.validator".to_string(),
                    summary: format!("validator dispatched: {title}"),
                    payload: json!({
                    "role": "validator",
                    "task": task_id,
                    "slice_id": slice_id,
                    "builder_batch_ref": builder_batch_ref
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
                "role": "validator",
                "executor": executor,
                "executor_profile": executor_profile,
                "executor_model": model,
                "expected_artifacts": ["validator_report"]
            },
            "mesh": mesh
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_dispatch_validator", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_dispatch_validator",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
