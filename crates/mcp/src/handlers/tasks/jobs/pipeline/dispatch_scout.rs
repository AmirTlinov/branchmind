#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_jobs_macro_dispatch_scout(&mut self, args: Value) -> Value {
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
                "target_ref",
                "objective",
                "constraints",
                "max_context_refs",
                "executor",
                "executor_profile",
                "model",
                "quality_profile",
                "novelty_policy",
                "critic_pass",
                "coverage_targets",
                "max_anchor_overlap",
                "max_ref_redundancy",
                "dry_run",
                "idempotency_key",
                "from_agent_id",
                "thread_id",
                "meta",
            ],
            "jobs.macro.dispatch.scout",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let planfs_target = match resolve_planfs_target_optional(self, &workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id_input = match optional_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match (slice_id_input, planfs_target.as_ref()) {
            (Some(value), Some(planfs)) => {
                if !value.eq_ignore_ascii_case(&planfs.slice_id) {
                    return ai_error(
                        "INVALID_INPUT",
                        "slice_id must match target_ref slice selector when both are provided",
                    );
                }
                value
            }
            (Some(value), None) => value,
            (None, Some(planfs)) => planfs.slice_id.clone(),
            (None, None) => {
                return ai_error("INVALID_INPUT", "slice_id or target_ref is required");
            }
        };
        let binding = match resolve_slice_binding_optional(self, &workspace, &slice_id) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if binding.is_none() && planfs_target.is_none() && self.jobs_slice_first_fail_closed_enabled
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "unknown slice_id: missing plan_slices binding (run tasks.slices.apply first)",
            );
        }

        let task_arg = match optional_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let focus_task = if task_arg.is_none() && binding.is_none() && planfs_target.is_some() {
            match self.store.focus_get(&workspace) {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            None
        };
        let task_id = match (task_arg, binding.as_ref(), focus_task) {
            (Some(v), _, _) => v,
            (None, Some(binding), _) => binding.plan_id.clone(),
            (None, None, Some(focus)) => {
                if matches!(
                    crate::support::parse_plan_or_task_kind(&focus),
                    Some(TaskKind::Task)
                ) {
                    focus
                } else {
                    return ai_error(
                        "INVALID_INPUT",
                        "task is required when target_ref is used without a focused TASK-*",
                    );
                }
            }
            (None, None, None) => {
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
                "jobs.macro.dispatch.scout: task must match slice binding plan_id (slice-first)",
            );
        }
        let anchor_id = match optional_non_empty_string(args_obj, "anchor") {
            Ok(v) => v.unwrap_or_else(|| format!("a:{}", slice_id.to_ascii_lowercase())),
            Err(resp) => return resp,
        };

        let requested_objective = match optional_non_empty_string(args_obj, "objective") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut constraints =
            match normalize_string_array(args_obj.get("constraints"), "constraints") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        let slice_first = binding.is_some();
        let objective = match (slice_first, requested_objective.clone(), binding.as_ref()) {
            (true, _requested, Some(binding)) => {
                if let Some(obj) = requested_objective.clone()
                    && obj != binding.spec.objective
                {
                    constraints.push(format!("requested_focus: {obj}"));
                }
                binding.spec.objective.clone()
            }
            (false, _requested, None) if planfs_target.is_some() => {
                let planfs = planfs_target.as_ref().expect("checked planfs target");
                if let Some(obj) = requested_objective.clone()
                    && obj != planfs.spec.objective
                {
                    constraints.push(format!("requested_focus: {obj}"));
                }
                planfs.spec.objective.clone()
            }
            (false, Some(obj), _) => obj,
            (false, None, _) => {
                return ai_error(
                    "INVALID_INPUT",
                    "objective is required when slice binding is missing (legacy/unplanned mode)",
                );
            }
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "internal error: missing slice binding objective",
                );
            }
        };
        let max_budget_refs = if let Some(binding) = binding.as_ref() {
            binding.spec.budgets.max_context_refs
        } else if let Some(planfs) = planfs_target.as_ref() {
            planfs.spec.budgets.max_context_refs
        } else {
            64
        };
        let max_context_refs_requested = match optional_usize(args_obj, "max_context_refs") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_context_refs_default = if slice_first || planfs_target.is_some() {
            max_budget_refs
        } else {
            24
        };
        let max_context_refs = max_context_refs_requested
            .unwrap_or(max_context_refs_default)
            .clamp(8, max_budget_refs);

        let slice_task_id_for_prompt = binding
            .as_ref()
            .map(|b| b.slice_task_id.clone())
            .unwrap_or_else(|| "-".to_string());
        let slice_spec = if let Some(binding) = binding.as_ref() {
            binding.spec.clone()
        } else if let Some(planfs) = planfs_target.as_ref() {
            let mut spec = planfs.spec.clone();
            spec.budgets.max_context_refs = max_context_refs;
            spec
        } else {
            let mut spec =
                crate::support::propose_next_slice_spec(&task_id, "", &objective, &constraints);
            // Legacy/unplanned: reflect the effective budget in the prompt for deterministic behavior.
            spec.budgets.max_context_refs = max_context_refs;
            spec
        };
        let executor = match optional_non_empty_string(args_obj, "executor") {
            Ok(v) => v
                .unwrap_or_else(|| DEFAULT_SCOUT_EXECUTOR.to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(executor.as_str(), "codex" | "claude_code") {
            return ai_error("INVALID_INPUT", "executor must be codex|claude_code");
        }
        let executor_profile = match optional_non_empty_string(args_obj, "executor_profile") {
            Ok(v) => v
                .unwrap_or_else(|| {
                    if executor == "claude_code" {
                        "deep".to_string()
                    } else {
                        DEFAULT_EXECUTOR_PROFILE.to_string()
                    }
                })
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(
            executor_profile.as_str(),
            "fast" | "deep" | "audit" | "xhigh"
        ) {
            return ai_error(
                "INVALID_INPUT",
                "executor_profile must be fast|deep|audit|xhigh",
            );
        }
        if executor == "claude_code" && executor_profile == "xhigh" {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.scout: claude_code scout does not support executor_profile=xhigh; use fast|deep|audit",
            );
        }
        let model = match optional_non_empty_string(args_obj, "model") {
            Ok(v) => v.unwrap_or_else(|| {
                if executor == "claude_code" {
                    DEFAULT_SCOUT_MODEL.to_string()
                } else {
                    DEFAULT_JOBS_MODEL.to_string()
                }
            }),
            Err(resp) => return resp,
        };
        if executor == "claude_code" && !model.to_ascii_lowercase().contains(DEFAULT_SCOUT_MODEL) {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.scout: claude_code scout model must be haiku-family",
            );
        }
        if executor == "codex" && !model.eq_ignore_ascii_case(DEFAULT_JOBS_MODEL) {
            return ai_error(
                "INVALID_INPUT",
                "jobs.macro.dispatch.scout: codex scout model must be gpt-5.3-codex",
            );
        }
        let quality_profile = match optional_non_empty_string(args_obj, "quality_profile") {
            Ok(v) => v
                .unwrap_or_else(|| "flagship".to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(quality_profile.as_str(), "standard" | "flagship") {
            return ai_error("INVALID_INPUT", "quality_profile must be standard|flagship");
        }
        let novelty_policy = match optional_non_empty_string(args_obj, "novelty_policy") {
            Ok(v) => v
                .unwrap_or_else(|| "strict".to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(novelty_policy.as_str(), "strict" | "warn") {
            return ai_error("INVALID_INPUT", "novelty_policy must be strict|warn");
        }
        let critic_pass = match optional_bool(args_obj, "critic_pass") {
            Ok(v) => v.unwrap_or(quality_profile == "flagship"),
            Err(resp) => return resp,
        };
        let max_anchor_overlap = match optional_ratio(args_obj, "max_anchor_overlap", 0.35) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_ref_redundancy = match optional_ratio(args_obj, "max_ref_redundancy", 0.25) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        // Flagship AI-DX: keep scout.test_hints bounded and non-noisy.
        // We derive the minimum from slice-level DoD tests only (not per-step checklists),
        // because task/step tests tend to be repetitive templates and would force scouts to
        // output long, low-signal test lists.
        let derived_test_target = slice_spec.dod.tests.len().clamp(3, 12);

        let mut coverage_targets = json!({
            "require_objective_coverage": true,
            "require_dod_coverage": true,
            "require_test_hints": derived_test_target,
            "require_risk_falsifier_pairs": 3
        });
        if let Some(overrides) = args_obj.get("coverage_targets") {
            let Some(coverage_obj) = overrides.as_object() else {
                return ai_error("INVALID_INPUT", "coverage_targets must be an object");
            };
            if let Some(dst) = coverage_targets.as_object_mut() {
                for (k, v) in coverage_obj {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
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

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        if let Some(obj) = requested_objective.as_deref()
            && obj != objective
        {
            warnings.push(warning(
                "OBJECTIVE_OVERRIDDEN",
                "objective arg ignored: slice objective is source of truth (kept as constraint requested_focus)",
                "Remove objective or set it equal to slice_plan_spec.objective.",
            ));
        }
        if let Some(req) = max_context_refs_requested
            && req != max_context_refs
        {
            if slice_first {
                warnings.push(warning(
                    "ARG_COERCED",
                    &format!(
                        "max_context_refs coerced to slice budget: requested={req}, effective={max_context_refs}"
                    ),
                    "Adjust slice_plan_spec.budgets.max_context_refs or omit max_context_refs.",
                ));
            } else {
                warnings.push(warning(
                    "ARG_COERCED",
                    &format!(
                        "max_context_refs coerced to safe range [8..{}]: requested={req}, effective={max_context_refs}",
                        max_budget_refs
                    ),
                    "Use max_context_refs in the supported range (or omit it).",
                ));
            }
        }
        if let Some(planfs) = planfs_target.as_ref()
            && binding.is_none()
        {
            warnings.push(warning(
                "PLANFS_TARGET_CONTEXT",
                &format!(
                    "using planfs target_ref context without plan_slices binding: {}",
                    planfs.target_ref
                ),
                "Run tasks.slices.apply to restore slice-first binding when you need strict step-tree determinism.",
            ));
        }

        let mut meta = args_obj
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        meta.insert("role".to_string(), Value::String("scout".to_string()));
        meta.insert(
            "dispatched_by".to_string(),
            Value::String("jobs.macro.dispatch.scout".to_string()),
        );
        meta.insert(
            "pipeline_role".to_string(),
            Value::String("scout".to_string()),
        );
        meta.insert("task".to_string(), Value::String(task_id.clone()));
        meta.insert("anchor".to_string(), Value::String(anchor_id.clone()));
        meta.insert("slice_id".to_string(), Value::String(slice_id.clone()));
        // Slice-first binds to a concrete slice task container; legacy/unplanned mode does not.
        if let Some(binding) = binding.as_ref() {
            meta.insert(
                "slice_task_id".to_string(),
                Value::String(binding.slice_task_id.clone()),
            );
        }
        if let Some(planfs) = planfs_target.as_ref() {
            meta.insert(
                "target_ref".to_string(),
                Value::String(planfs.target_ref.clone()),
            );
            meta.insert(
                "planfs_slug".to_string(),
                Value::String(planfs.plan_slug.clone()),
            );
            meta.insert(
                "planfs_path".to_string(),
                Value::String(planfs.plan_path.clone()),
            );
            meta.insert(
                "planfs_slice_file".to_string(),
                Value::String(planfs.slice_file.clone()),
            );
            meta.insert(
                "planfs_excerpt".to_string(),
                Value::String(planfs.excerpt.clone()),
            );
            meta.insert(
                "planfs_excerpt_chars".to_string(),
                json!(planfs.excerpt.chars().count()),
            );
        }
        meta.insert("plan_id".to_string(), Value::String(task_id.clone()));
        meta.insert("objective".to_string(), Value::String(objective.clone()));
        meta.insert(
            "constraints".to_string(),
            Value::Array(constraints.iter().cloned().map(Value::String).collect()),
        );
        meta.insert(
            "max_context_refs".to_string(),
            Value::Number(serde_json::Number::from(max_context_refs as u64)),
        );
        meta.insert("executor".to_string(), Value::String(executor.clone()));
        meta.insert(
            "executor_profile".to_string(),
            Value::String(executor_profile.clone()),
        );
        meta.insert("executor_model".to_string(), Value::String(model.clone()));
        meta.insert(
            "quality_profile".to_string(),
            Value::String(quality_profile.clone()),
        );
        meta.insert(
            "novelty_policy".to_string(),
            Value::String(novelty_policy.clone()),
        );
        meta.insert("critic_pass".to_string(), Value::Bool(critic_pass));
        meta.insert("max_anchor_overlap".to_string(), json!(max_anchor_overlap));
        meta.insert("max_ref_redundancy".to_string(), json!(max_ref_redundancy));
        meta.insert("coverage_targets".to_string(), coverage_targets.clone());
        meta.insert(
            "expected_artifacts".to_string(),
            Value::Array(vec![Value::String("scout_context_pack".to_string())]),
        );
        let mut pipeline = json!({
            "task": task_id,
            "anchor": anchor_id,
            "slice_id": slice_id,
            "objective": objective,
            "constraints": constraints,
            "max_context_refs": max_context_refs,
            "quality_profile": quality_profile,
            "novelty_policy": novelty_policy,
            "critic_pass": critic_pass,
            "coverage_targets": coverage_targets
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
        if let Some(planfs) = planfs_target.as_ref()
            && let Some(obj) = pipeline.as_object_mut()
        {
            obj.insert(
                "target_ref".to_string(),
                Value::String(planfs.target_ref.clone()),
            );
            obj.insert(
                "planfs_path".to_string(),
                Value::String(planfs.plan_path.clone()),
            );
            obj.insert(
                "planfs_slice_file".to_string(),
                Value::String(planfs.slice_file.clone()),
            );
            obj.insert(
                "planfs_excerpt_chars".to_string(),
                json!(planfs.excerpt.chars().count()),
            );
        }
        meta.insert("pipeline".to_string(), pipeline);
        let meta_json = serde_json::to_string(&Value::Object(meta)).ok();
        let title = format!("Scout context for {slice_id}");
        let priority = "MEDIUM".to_string();
        let constraints_text = if constraints.is_empty() {
            "- (none)".to_string()
        } else {
            constraints
                .iter()
                .map(|v| format!("- {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let slice_plan_spec_text = serde_json::to_string_pretty(&slice_spec.to_json())
            .or_else(|_| serde_json::to_string(&slice_spec.to_json()))
            .unwrap_or_else(|_| "{}".to_string());
        let coverage_targets_text = serde_json::to_string_pretty(&coverage_targets)
            .or_else(|_| serde_json::to_string(&coverage_targets))
            .unwrap_or_else(|_| "{}".to_string());
        let planfs_prompt_block = if let Some(planfs) = planfs_target.as_ref() {
            format!(
                "PlanFS target_ref: {}\nPlanFS path: {}/{}\nPlanFS slice excerpt (bounded):\n{}\n\n",
                planfs.target_ref, planfs.plan_path, planfs.slice_file, planfs.excerpt
            )
        } else {
            String::new()
        };
        let quality_clause = if quality_profile == "flagship" {
            format!(
                "QUALITY PROFILE=flagship (fail-closed).\n\
Thresholds: anchors>=3, change_hints>=2, test_hints>=3, risk_map>=3, summary_for_builder>=320 chars.\n\
MUST deduplicate aggressively (anchors/code_refs/change_hints/test_hints/risk_map): avoid repeating the same file+intent.\n\
Novelty gates: anchor_uniqueness>=0.80, ref_redundancy<={max_ref_redundancy:.2}, anchor_overlap<={max_anchor_overlap:.2}.\n\
Novelty policy={novelty_policy}; critic_pass={critic_pass}.\n"
            )
        } else {
            "QUALITY PROFILE=standard.\nThresholds: anchors>=3, change_hints>=2, test_hints>=2, risk_map>=2, summary_for_builder>=240 chars.\n".to_string()
        };
        let role_prompt = format!(
            "ROLE=SCOUT\n\
MUST output ONLY scout_context_pack JSON.\n\
MUST NOT output code, patch, diff, apply instructions.\n\
MUST include uncertainty and falsifiers.\n\
MUST keep context extraction bounded: max 12 repository reads; stop immediately when coverage_targets are satisfied.\n\
MUST deduplicate aggressively: no repeated file+intent pairs in anchors/code_refs/change_hints.\n\
MUST format every code_refs[] as CODE_REF: code:<repo_rel>#L<start>-L<end> (optional @sha256:<64hex>; BranchMind normalizes).\n\
MUST NOT invent file paths: only cite files that exist under the workspace repo root.\n\
MUST emit typed anchors: every anchors[] item includes anchor_type (primary|dependency|reference|structural) + code_ref.\n\
{quality_clause}\
Execution target: executor={executor} model={model} profile={executor_profile}.\n\n\
Plan: {task_id}\nSlice: {slice_id}\nSlice task: {}\nAnchor: {anchor_id}\nObjective (slice): {objective}\nmax_context_refs: {max_context_refs}\n\n\
{planfs_prompt_block}\
SlicePlanSpec (source of truth, bounded):\n{slice_plan_spec_text}\n\n\
Constraints:\n{constraints_text}\n\n\
Coverage targets:\n{coverage_targets_text}\n",
            slice_task_id_for_prompt
        );
        let job_kind = if executor == "claude_code" {
            "claude_cli"
        } else {
            "codex_cli"
        };

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
                    anchor_id: Some(anchor_id.clone()),
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
                    kind: "dispatch.scout".to_string(),
                    summary: format!("scout dispatched: {title}"),
                    payload: json!({
                    "role": "scout",
                    "task": task_id,
                    "anchor": anchor_id,
                    "slice_id": slice_id,
                    "objective": objective,
                    "executor": executor.clone(),
                    "executor_profile": executor_profile.clone(),
                    "model": model.clone()
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
                "executor": executor,
                "executor_profile": executor_profile,
                "executor_model": model
            },
            "expected_artifacts": ["scout_context_pack"],
            "planfs": planfs_target.as_ref().map(|planfs| json!({
                "target_ref": planfs.target_ref,
                "slug": planfs.plan_slug,
                "path": planfs.plan_path,
                "slice_file": planfs.slice_file,
                "slice_id": planfs.slice_id,
                "excerpt_chars": planfs.excerpt.chars().count()
            })).unwrap_or(Value::Null),
            "mesh": mesh
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_dispatch_scout", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_dispatch_scout",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
