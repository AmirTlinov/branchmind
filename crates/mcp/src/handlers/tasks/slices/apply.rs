#![forbid(unsafe_code)]

use super::*;

impl McpServer {
    pub(crate) fn tool_tasks_slices_apply(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if let Err(resp) = check_unknown_args(
            args_obj,
            &[
                "workspace",
                "plan",
                "task",
                "expected_revision",
                "policy",
                "slice_plan_spec",
            ],
            "tasks.slices.apply",
        ) {
            return resp;
        }
        if !self.slice_plans_v1_enabled {
            return ai_error_with(
                "FEATURE_DISABLED",
                "slice_plans_v1 is disabled",
                Some("Enable via --slice-plans-v1 (or env BRANCHMIND_SLICE_PLANS_V1=1)."),
                Vec::new(),
            );
        }
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (plan_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj)
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !matches!(kind, TaskKind::Plan) {
            return ai_error("INVALID_INPUT", "plan is required");
        }
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let policy = match optional_string(args_obj, "policy") {
            Ok(v) => v.unwrap_or_else(|| "fail_closed".to_string()),
            Err(resp) => return resp,
        };
        if !policy.eq_ignore_ascii_case("fail_closed") {
            return ai_error("INVALID_INPUT", "policy must be fail_closed");
        }
        let raw_spec = match args_obj.get("slice_plan_spec") {
            Some(v) => v,
            None => return ai_error("INVALID_INPUT", "slice_plan_spec is required"),
        };
        let spec = match crate::support::parse_slice_plan_spec(raw_spec) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let plan = match self.store.get_plan(&workspace, &plan_id) {
            Ok(Some(p)) => p,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if let Some(exp) = expected_revision
            && exp != plan.revision
        {
            return ai_error_with(
                "REVISION_MISMATCH",
                "plan revision mismatch",
                Some("Refresh plan and retry with the new expected_revision."),
                vec![json!({ "expected_revision": exp, "actual_revision": plan.revision })],
            );
        }

        let slice_id = match self.store.plan_slice_next_id(&workspace) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let slice_task_title = format!("{} — {}", slice_id, spec.title);
        let spec_json = serde_json::to_string(&spec.to_json()).unwrap_or_else(|_| "{}".to_string());
        let budgets_json = serde_json::to_string(&spec.budgets.to_json()).ok();
        let (slice_task_id, _rev, _event) = match self.store.create(
            &workspace,
            bm_storage::TaskCreateRequest {
                kind: TaskKind::Task,
                title: slice_task_title.clone(),
                parent_plan_id: Some(plan_id.clone()),
                description: Some(spec.objective.clone()),
                contract: None,
                contract_json: None,
                event_type: "slice_created".to_string(),
                event_payload_json: json!({
                    "plan_id": plan_id,
                    "slice_id": slice_id,
                    "title": slice_task_title,
                    "objective": spec.objective,
                })
                .to_string(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Store canonical SlicePlanSpec JSON in task.context (tasks table has no contract_json).
        let _ = match self.store.edit_task(
            &workspace,
            bm_storage::TaskEditRequest {
                id: slice_task_id.clone(),
                expected_revision: None,
                title: None,
                description: None,
                context: Some(Some(spec_json.clone())),
                priority: None,
                domain: None,
                reasoning_mode: None,
                phase: None,
                component: None,
                assignee: None,
                tags: None,
                depends_on: None,
                event_type: "slice_spec_set".to_string(),
                event_payload_json: json!({
                    "plan_id": plan_id,
                    "slice_id": slice_id,
                    "slice_task_id": slice_task_id,
                    "format": "slice_plan_spec.v1"
                })
                .to_string(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Root steps = SliceTasks.
        let root_steps = spec
            .tasks
            .iter()
            .map(|t| bm_storage::NewStep {
                title: t.title.clone(),
                success_criteria: t.success_criteria.clone(),
            })
            .collect::<Vec<_>>();
        let root =
            match self
                .store
                .steps_decompose(&workspace, &slice_task_id, None, None, root_steps)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        // Define root step tests/blockers.
        for (idx, step_ref) in root.steps.iter().enumerate() {
            let task_spec = &spec.tasks[idx];
            let result = self.store.step_define(
                &workspace,
                bm_storage::StepDefineRequest {
                    task_id: slice_task_id.clone(),
                    expected_revision: None,
                    agent_id: self.default_agent_id.clone(),
                    selector: bm_storage::StepSelector {
                        step_id: Some(step_ref.step_id.clone()),
                        path: None,
                    },
                    patch: bm_storage::StepPatch {
                        title: None,
                        success_criteria: None,
                        tests: Some(task_spec.tests.clone()),
                        blockers: Some(task_spec.blockers.clone()),
                        next_action: None,
                        stop_criteria: None,
                        proof_tests_mode: None,
                        proof_security_mode: None,
                        proof_perf_mode: None,
                        proof_docs_mode: None,
                    },
                },
            );
            match result {
                Ok(_) => {}
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        }

        // Child steps = Steps per SliceTask.
        for (idx, root_ref) in root.steps.iter().enumerate() {
            let task_spec = &spec.tasks[idx];
            let parent_path = match StepPath::parse(&root_ref.path) {
                Ok(v) => v,
                Err(_) => {
                    return ai_error(
                        "PRECONDITION_FAILED",
                        "internal error: invalid step path for newly created root step",
                    );
                }
            };
            let child_steps = task_spec
                .steps
                .iter()
                .map(|s| bm_storage::NewStep {
                    title: s.title.clone(),
                    success_criteria: s.success_criteria.clone(),
                })
                .collect::<Vec<_>>();
            let decomp = match self.store.steps_decompose(
                &workspace,
                &slice_task_id,
                None,
                Some(&parent_path),
                child_steps,
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            for (step_idx, step_ref) in decomp.steps.iter().enumerate() {
                let step_spec = &task_spec.steps[step_idx];
                let define = self.store.step_define(
                    &workspace,
                    bm_storage::StepDefineRequest {
                        task_id: slice_task_id.clone(),
                        expected_revision: None,
                        agent_id: self.default_agent_id.clone(),
                        selector: bm_storage::StepSelector {
                            step_id: Some(step_ref.step_id.clone()),
                            path: None,
                        },
                        patch: bm_storage::StepPatch {
                            title: None,
                            success_criteria: None,
                            tests: Some(step_spec.tests.clone()),
                            blockers: Some(step_spec.blockers.clone()),
                            next_action: None,
                            stop_criteria: None,
                            proof_tests_mode: None,
                            proof_security_mode: None,
                            proof_perf_mode: None,
                            proof_docs_mode: None,
                        },
                    },
                );
                match define {
                    Ok(_) => {}
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            }
        }

        let binding = match self.store.plan_slice_insert(
            &workspace,
            bm_storage::PlanSliceInsertRequest {
                plan_id: plan_id.clone(),
                slice_id: slice_id.clone(),
                slice_task_id: slice_task_id.clone(),
                title: spec.title.clone(),
                objective: spec.objective.clone(),
                status: "planned".to_string(),
                budgets_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown plan/task id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let actions =
            slice_actions_after_apply(workspace.as_str(), &plan_id, &slice_id, &spec.objective);
        ai_ok(
            "tasks_slices_apply",
            json!({
                "workspace": workspace.as_str(),
                "plan_id": plan_id,
                "slice": {
                    "plan_id": binding.plan_id,
                    "slice_id": binding.slice_id,
                    "slice_task_id": binding.slice_task_id,
                    "title": binding.title,
                    "objective": binding.objective,
                    "status": binding.status,
                    "budgets": spec.budgets.to_json()
                },
                "slice_plan_spec": spec.to_json(),
                "actions": actions
            }),
        )
    }
}
