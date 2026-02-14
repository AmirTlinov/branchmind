#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_jobs_macro_dispatch_builder(&mut self, args: Value) -> Value {
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
                "allow_prevalidate_non_pass",
                "strict_scout_mode",
                "scout_stale_after_s",
                "context_quality_gate",
                "input_mode",
                "max_context_requests",
                "context_retry_count",
                "context_retry_limit",
            ],
            "jobs.macro.dispatch.builder",
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
                "jobs.macro.dispatch.builder: task must match slice binding plan_id (slice-first)",
            );
        }
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
        let requested_objective = match optional_non_empty_string(args_obj, "objective") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let objective = if let Some(binding) = binding.as_ref() {
            binding.spec.objective.clone()
        } else if let Some(obj) = requested_objective.clone() {
            obj
        } else {
            return ai_error(
                "INVALID_INPUT",
                "objective is required when slice binding is missing (legacy/unplanned mode)",
            );
        };
        let mut dod = match args_obj.get("dod") {
            Some(v) if v.is_object() => v.clone(),
            Some(_) => return ai_error("INVALID_INPUT", "dod must be an object"),
            None => {
                if slice_first {
                    json!({})
                } else {
                    return ai_error(
                        "INVALID_INPUT",
                        "dod is required when slice binding is missing (legacy/unplanned mode)",
                    );
                }
            }
        };
        let budgets_json = if let Some(binding) = binding.as_ref() {
            binding.spec.budgets.to_json()
        } else if let Some(budgets) = dod.get("budgets") {
            if !budgets.is_object() {
                return ai_error("INVALID_INPUT", "dod.budgets must be an object");
            }
            budgets.clone()
        } else {
            crate::support::SliceBudgets::default().to_json()
        };

        let slice_task_id_for_prompt = binding
            .as_ref()
            .map(|b| b.slice_task_id.clone())
            .unwrap_or_else(|| "-".to_string());
        let slice_spec = if let Some(binding) = binding.as_ref() {
            binding.spec.clone()
        } else {
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
            spec
        };
        if let Some(obj) = dod.as_object_mut() {
            // Slice-first: fill DoD defaults from slice_plan_spec unless caller already provided them.
            if slice_first {
                obj.entry("criteria".to_string()).or_insert(Value::Array(
                    slice_spec
                        .dod
                        .criteria
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ));
                obj.entry("tests".to_string()).or_insert(Value::Array(
                    slice_spec
                        .dod
                        .tests
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ));
                obj.entry("blockers".to_string()).or_insert(Value::Array(
                    slice_spec
                        .dod
                        .blockers
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ));
            }

            obj.entry("security".to_string())
                .or_insert(Value::Array(Vec::new()));
            obj.entry("budgets".to_string())
                .or_insert(budgets_json.clone());

            if !slice_first {
                let tests_ok = obj
                    .get("tests")
                    .and_then(|v| v.as_array())
                    .is_some_and(|arr| !arr.is_empty());
                if !tests_ok {
                    return ai_error(
                        "INVALID_INPUT",
                        "dod.tests is required and must be a non-empty array (legacy/unplanned mode)",
                    );
                }
                let blockers_ok = obj
                    .get("blockers")
                    .and_then(|v| v.as_array())
                    .is_some_and(|arr| !arr.is_empty());
                if !blockers_ok {
                    return ai_error(
                        "INVALID_INPUT",
                        "dod.blockers is required and must be a non-empty array (legacy/unplanned mode)",
                    );
                }
            }
        }

        // Fail-closed: in slice-first mode, builder can only run on a validated, deterministic slice plan.
        if slice_first {
            let Some(binding) = binding.as_ref() else {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "internal error: slice_first without binding",
                );
            };
            let slice_steps =
                match self
                    .store
                    .list_task_steps(&workspace, &binding.slice_task_id, None, 400)
                {
                    Ok(v) => v,
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
            if let Err(resp) = crate::support::validate_slice_step_tree(&slice_steps, &slice_spec) {
                return resp;
            }
        }
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let allow_prevalidate_non_pass = match optional_bool(args_obj, "allow_prevalidate_non_pass")
        {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let strict_scout_mode = match optional_bool(args_obj, "strict_scout_mode") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let context_quality_gate = match optional_bool(args_obj, "context_quality_gate") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let input_mode = match optional_non_empty_string(args_obj, "input_mode") {
            Ok(v) => v
                .unwrap_or_else(|| "strict".to_string())
                .trim()
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(input_mode.as_str(), "strict" | "flex") {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: input_mode must be strict|flex",
            );
        }
        let scout_stale_after_s = match optional_i64(args_obj, "scout_stale_after_s") {
            Ok(v) => v
                .unwrap_or(DEFAULT_STRICT_SCOUT_STALE_AFTER_S)
                .clamp(1, 86_400),
            Err(resp) => return resp,
        };
        let max_context_requests = match optional_usize(args_obj, "max_context_requests") {
            Ok(v) => v.unwrap_or(MAX_CONTEXT_RETRY_LIMIT as usize),
            Err(resp) => return resp,
        };
        let context_retry_count = match optional_usize(args_obj, "context_retry_count") {
            Ok(v) => (v.unwrap_or(0) as u64).min(MAX_CONTEXT_RETRY_LIMIT),
            Err(resp) => return resp,
        };
        let context_retry_limit = match optional_usize(args_obj, "context_retry_limit") {
            Ok(v) => {
                let requested = v.unwrap_or(max_context_requests);
                (requested as u64).min(MAX_CONTEXT_RETRY_LIMIT)
            }
            Err(resp) => return resp,
        };
        if context_retry_count > context_retry_limit {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: context_retry_count must be <= context_retry_limit",
            );
        }
        if strict_scout_mode && allow_prevalidate_non_pass {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: allow_prevalidate_non_pass is forbidden when strict_scout_mode=true",
            );
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
                .unwrap_or_else(|| "codex".to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !executor.eq_ignore_ascii_case("codex") {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: executor must be codex",
            );
        }
        let executor_profile = match optional_non_empty_string(args_obj, "executor_profile") {
            Ok(v) => v
                .unwrap_or_else(|| DEFAULT_EXECUTOR_PROFILE.to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !executor_profile.eq_ignore_ascii_case(DEFAULT_EXECUTOR_PROFILE) {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: executor_profile must be xhigh",
            );
        }
        let model = match optional_non_empty_string(args_obj, "model") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_JOBS_MODEL.to_string()),
            Err(resp) => return resp,
        };
        if !model.eq_ignore_ascii_case(DEFAULT_JOBS_MODEL) {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.builder: model must be gpt-5.3-codex",
            );
        }

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        if let Some(obj) = requested_objective.as_deref()
            && obj != objective
        {
            warnings.push(warning(
                "OBJECTIVE_OVERRIDDEN",
                "objective arg ignored: slice objective is source of truth",
                "Remove objective or set it equal to slice_plan_spec.objective.",
            ));
        }
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
                "jobs.macro.dispatch.builder: scout job is not DONE",
            );
        }
        let now_ms = now_ms_i64();
        let scout_age_ms = now_ms.saturating_sub(scout_open.job.updated_at_ms);
        if strict_scout_mode && scout_age_ms > scout_stale_after_s.saturating_mul(1_000) {
            return ai_error(
                "PRECONDITION_FAILED",
                &format!(
                    "jobs.macro.dispatch.builder: scout pack is stale (age_s={}, stale_after_s={})",
                    scout_age_ms / 1_000,
                    scout_stale_after_s
                ),
            );
        }
        let scout_summary = match scout_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.macro.dispatch.builder: scout job summary is empty",
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
        if strict_scout_mode
            && matches!(scout_policy.quality_profile, ScoutQualityProfile::Standard)
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.macro.dispatch.builder: strict_scout_mode requires scout quality_profile=flagship",
            );
        }
        if strict_scout_mode && !matches!(scout_policy.novelty_policy, ScoutNoveltyPolicy::Strict) {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.macro.dispatch.builder: strict_scout_mode requires scout novelty_policy=strict",
            );
        }
        let (scout_norm, scout_contract_warnings) = match validate_scout_context_pack_contract(
            &self.store,
            &workspace,
            &scout_json,
            scout_max_context_refs,
            &scout_policy,
        ) {
            Ok(v) => v,
            Err(resp) => {
                let detail = resp
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("invalid scout context pack");
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "jobs.macro.dispatch.builder: scout pack failed strict quality contract: {detail}"
                    ),
                );
            }
        };
        if context_quality_gate {
            let first_stale = scout_contract_warnings.iter().find(|warning| {
                matches!(
                    warning.get("code").and_then(|v| v.as_str()),
                    Some("CODE_REF_STALE" | "CODE_REF_MISSING" | "CODE_REF_RANGE_STALE")
                )
            });
            if let Some(stale) = first_stale {
                let detail = stale
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stale CODE_REF warning");
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "jobs.macro.dispatch.builder: strict context quality gate rejected stale scout CODE_REF: {detail}"
                    ),
                );
            }
            if strict_scout_mode {
                let first_strict_warning = scout_contract_warnings.iter().find(|warning| {
                    !matches!(
                        warning.get("code").and_then(|v| v.as_str()),
                        Some("CODE_REF_UNRESOLVABLE")
                    )
                });
                if let Some(first_warning) = first_strict_warning {
                    let detail = first_warning
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("scout contract warning under strict mode");
                    return ai_error(
                        "PRECONDITION_FAILED",
                        &format!(
                            "jobs.macro.dispatch.builder: strict scout quality gate rejected warning-level scout pack: {detail}"
                        ),
                    );
                }
            }
        }
        warnings.extend(scout_contract_warnings);
        let scout_anchors_v2 = parse_scout_anchors_v2(&scout_norm);
        let (pre_verdict, pre_checks) = pre_validate_scout_pack(&scout_norm, &scout_anchors_v2);
        match pre_verdict {
            PreValidatorVerdict::Pass => {}
            PreValidatorVerdict::NeedMore { hints } => {
                if strict_scout_mode || !allow_prevalidate_non_pass {
                    let detail = if hints.is_empty() {
                        "pre-validate returned need_more with empty hints".to_string()
                    } else {
                        hints.join("; ")
                    };
                    return ai_error(
                        "PRECONDITION_FAILED",
                        &format!(
                            "jobs.macro.dispatch.builder: scout pre-validate is not PASS: {detail}"
                        ),
                    );
                }
                warnings.push(json!({
                    "code": "SCOUT_PREVALIDATE_NEED_MORE",
                    "message": if hints.is_empty() {
                        "scout pre-validate returned need_more with no hints".to_string()
                    } else {
                        format!("scout pre-validate need_more: {}", hints.join("; "))
                    },
                    "recovery": "Refresh scout context pack (preferred) or keep allow_prevalidate_non_pass=true to proceed."
                }));
            }
            PreValidatorVerdict::Reject { reason } => {
                if strict_scout_mode || !allow_prevalidate_non_pass {
                    return ai_error(
                        "PRECONDITION_FAILED",
                        &format!(
                            "jobs.macro.dispatch.builder: scout pre-validate rejected context: {reason}"
                        ),
                    );
                }
                warnings.push(json!({
                    "code": "SCOUT_PREVALIDATE_REJECT",
                    "message": format!("scout pre-validate rejected context: {reason}"),
                    "recovery": "Dispatch a stronger scout (recommended) and retry builder."
                }));
            }
        }

        let mut meta = args_obj
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        meta.insert("role".to_string(), Value::String("builder".to_string()));
        meta.insert(
            "pipeline_role".to_string(),
            Value::String("builder".to_string()),
        );
        meta.insert(
            "dispatched_by".to_string(),
            Value::String("jobs.macro.dispatch.builder".to_string()),
        );
        meta.insert("task".to_string(), Value::String(task_id.clone()));
        meta.insert("slice_id".to_string(), Value::String(slice_id.clone()));
        if let Some(binding) = binding.as_ref() {
            meta.insert(
                "slice_task_id".to_string(),
                Value::String(binding.slice_task_id.clone()),
            );
        }
        meta.insert("plan_id".to_string(), Value::String(task_id.clone()));
        meta.insert("slice_budgets".to_string(), budgets_json.clone());
        meta.insert(
            "scout_pack_ref".to_string(),
            Value::String(scout_pack_ref.clone()),
        );
        meta.insert("scout_job_id".to_string(), Value::String(scout_job_id));
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
            Value::Array(vec![Value::String("builder_diff_batch".to_string())]),
        );
        let mut pipeline = json!({
            "task": task_id,
            "slice_id": slice_id,
            "scout_pack_ref": scout_pack_ref
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
        meta.insert(
            "scout_prevalidate".to_string(),
            json!({
                "allow_non_pass": allow_prevalidate_non_pass,
                "checks": {
                    "completeness_ok": pre_checks.completeness_ok,
                    "dependencies_ok": pre_checks.dependencies_ok,
                    "patterns_ok": pre_checks.patterns_ok,
                    "intent_coverage_ok": pre_checks.intent_coverage_ok
                }
            }),
        );
        meta.insert(
            "strict_scout_mode".to_string(),
            Value::Bool(strict_scout_mode),
        );
        meta.insert(
            "context_quality_gate".to_string(),
            Value::Bool(context_quality_gate),
        );
        meta.insert("input_mode".to_string(), Value::String(input_mode.clone()));
        meta.insert(
            "scout_stale_after_s".to_string(),
            json!(scout_stale_after_s),
        );
        meta.insert("scout_age_s".to_string(), json!(scout_age_ms / 1_000));
        meta.insert(
            "context_retry_count".to_string(),
            json!(context_retry_count),
        );
        meta.insert(
            "context_retry_limit".to_string(),
            json!(context_retry_limit),
        );
        meta.insert(
            "max_context_requests".to_string(),
            json!(context_retry_limit),
        );
        let meta_json = serde_json::to_string(&Value::Object(meta)).ok();
        let title = format!("Builder diff for {slice_id}");
        let priority = "MEDIUM".to_string();
        let dod_text = serde_json::to_string_pretty(&dod)
            .or_else(|_| serde_json::to_string(&dod))
            .unwrap_or_else(|_| "{}".to_string());
        let slice_task_id = slice_task_id_for_prompt.clone();
        let slice_plan_spec_text = serde_json::to_string_pretty(&slice_spec.to_json())
            .or_else(|_| serde_json::to_string(&slice_spec.to_json()))
            .unwrap_or_else(|_| "{}".to_string());
        let budgets_text = serde_json::to_string_pretty(&budgets_json)
            .or_else(|_| serde_json::to_string(&budgets_json))
            .unwrap_or_else(|_| "{}".to_string());
        let role_prompt = format!(
            "ROLE=BUILDER\n\
MUST use only scout_context_pack + slice_plan_spec (provided below).\n\
MUST output ONLY builder_diff_batch JSON.\n\
MUST include proof_refs, rollback_plan, and execution_evidence{{revision,diff_scope,command_runs,rollback_proof,semantic_guards}}.\n\
MUST ensure every proof_refs[] entry starts with CMD:/LINK:/FILE:.\n\
MUST shape execution_evidence.command_runs[] as {{cmd,exit_code,stdout_ref,stderr_ref}}.\n\
MUST shape execution_evidence.rollback_proof as {{strategy,target_revision,verification_cmd_ref}}.\n\
MUST shape execution_evidence.semantic_guards as {{must_should_may_delta,contract_term_consistency}}.\n\
Input mode: {input_mode}.\n\
If input_mode=strict, MUST NOT call MCP tools / repo discovery loops; use only provided context.\n\
If scout context is insufficient, you MAY return context_request={{reason,missing_context[],suggested_scout_focus[],suggested_tests[]}} and set changes=[].\n\
Context request loop budget: context_retry_count={context_retry_count}, context_retry_limit={context_retry_limit} (must stay <=2).\n\
MUST NOT skip tests listed in DoD unless explicit EXCEPTION.\n\
Execution target: model=gpt-5.3-codex profile=xhigh.\n\n\
Plan: {task_id}\nSlice: {slice_id}\nSlice task: {slice_task_id}\nObjective (slice): {objective}\nScout pack ref: {scout_pack_ref}\n\n\
Budgets (fail-closed):\n{budgets_text}\n\n\
SlicePlanSpec (source of truth, bounded):\n{slice_plan_spec_text}\n\n\
Strict scout mode: {strict_scout_mode} (stale_after_s={scout_stale_after_s}, scout_age_s={}).\n\
Context quality gate: {context_quality_gate}.\n\n\
DoD:\n{dod_text}\n",
            scout_age_ms / 1_000
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
                    kind: "dispatch.builder".to_string(),
                    summary: format!("builder dispatched: {title}"),
                    payload: json!({
                    "role": "builder",
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
                "role": "builder",
                "executor": executor,
                "executor_profile": executor_profile,
                "executor_model": model,
                "expected_artifacts": ["builder_diff_batch"],
                "strict_scout_mode": strict_scout_mode,
                "scout_stale_after_s": scout_stale_after_s,
                "context_retry_count": context_retry_count,
                "context_retry_limit": context_retry_limit
            },
            "mesh": mesh
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_dispatch_builder", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_dispatch_builder",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
