#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) struct BootstrapStepInput {
    pub title: String,
    pub criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub proof_tests_mode: bm_storage::ProofMode,
    pub proof_security_mode: bm_storage::ProofMode,
    pub proof_perf_mode: bm_storage::ProofMode,
    pub proof_docs_mode: bm_storage::ProofMode,
}

pub(super) struct TasksBootstrapArgs {
    pub workspace: WorkspaceId,
    pub plan_id: Option<String>,
    pub parent_id: Option<String>,
    pub plan_title: Option<String>,
    pub plan_template: Option<String>,
    pub task_title: String,
    pub description: Option<String>,
    pub steps: Vec<BootstrapStepInput>,
    pub think: Option<Value>,
}

pub(super) fn parse_tasks_bootstrap_args(args: Value) -> Result<TasksBootstrapArgs, Value> {
    let Some(args_obj) = args.as_object() else {
        return Err(ai_error("INVALID_INPUT", "arguments must be an object"));
    };
    let workspace = require_workspace(args_obj)?;

    let plan_id = args_obj
        .get("plan")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let parent_id = args_obj
        .get("parent")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if plan_id.is_some() && parent_id.is_some() {
        return Err(ai_error(
            "INVALID_INPUT",
            "provide plan or parent, not both",
        ));
    }

    let plan_title = optional_string(args_obj, "plan_title")?;
    let plan_template = optional_string(args_obj, "plan_template")?;
    if plan_template.is_some() {
        if plan_title.is_none() {
            return Err(ai_error(
                "INVALID_INPUT",
                "plan_template requires plan_title",
            ));
        }
        if plan_id.is_some() || parent_id.is_some() {
            return Err(ai_error(
                "INVALID_INPUT",
                "plan_template requires creating a new plan; provide plan_title without plan/parent",
            ));
        }
    }
    let task_title = require_string(args_obj, "task_title")?;
    let description = optional_string(args_obj, "description")?;

    let template_id = optional_string(args_obj, "template")?;
    let steps = match template_id {
        Some(id) => {
            let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
            if !steps_value.is_null() {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "provide steps or template, not both",
                ));
            }

            let template = match find_task_template(&id, TaskKind::Task) {
                Some(v) => v,
                None => {
                    return Err(ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown template id",
                        Some(
                            "Use built-in template ids: basic-task or principal-task. (Use tasks_templates_list.)",
                        ),
                        vec![suggest_call(
                            "tasks_templates_list",
                            "List available templates.",
                            "high",
                            serde_json::json!({ "workspace": workspace.as_str() }),
                        )],
                    ));
                }
            };
            if template.task_steps.is_empty() {
                return Err(ai_error("INVALID_INPUT", "template has no task steps"));
            }

            template
                .task_steps
                .into_iter()
                .map(|step| BootstrapStepInput {
                    title: step.title,
                    criteria: step.success_criteria,
                    tests: step.tests,
                    blockers: step.blockers,
                    proof_tests_mode: step.proof_tests_mode,
                    proof_security_mode: step.proof_security_mode,
                    proof_perf_mode: step.proof_perf_mode,
                    proof_docs_mode: step.proof_docs_mode,
                })
                .collect::<Vec<_>>()
        }
        None => {
            let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
            if steps_value.is_null() {
                return Err(ai_error("INVALID_INPUT", "provide steps or template"));
            }
            parse_bootstrap_steps(args_obj)?
        }
    };
    let think = args_obj.get("think").cloned().filter(|v| !v.is_null());

    Ok(TasksBootstrapArgs {
        workspace,
        plan_id,
        parent_id,
        plan_title,
        plan_template,
        task_title,
        description,
        steps,
        think,
    })
}

fn parse_bootstrap_steps(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<Vec<BootstrapStepInput>, Value> {
    let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
    let Some(steps_array) = steps_value.as_array() else {
        return Err(ai_error("INVALID_INPUT", "steps must be an array"));
    };
    if steps_array.is_empty() {
        return Err(ai_error("INVALID_INPUT", "steps must not be empty"));
    }

    let mut steps = Vec::with_capacity(steps_array.len());
    for step in steps_array {
        let Some(obj) = step.as_object() else {
            return Err(ai_error("INVALID_INPUT", "steps[] items must be objects"));
        };

        let title = require_string(obj, "title")?;

        let criteria = {
            let criteria_value = obj.get("success_criteria").cloned().unwrap_or(Value::Null);
            let Some(criteria_array) = criteria_value.as_array() else {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "steps[].success_criteria must be an array",
                ));
            };
            if criteria_array.is_empty() {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "steps[].success_criteria must not be empty",
                ));
            }
            let mut criteria = Vec::with_capacity(criteria_array.len());
            for item in criteria_array {
                let Some(s) = item.as_str() else {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        "steps[].success_criteria items must be strings",
                    ));
                };
                criteria.push(s.to_string());
            }
            normalize_required_string_list(criteria, "steps[].success_criteria")?
        };

        let tests = {
            let tests_value = obj.get("tests").cloned().unwrap_or(Value::Null);
            let Some(tests_array) = tests_value.as_array() else {
                return Err(ai_error("INVALID_INPUT", "steps[].tests must be an array"));
            };
            if tests_array.is_empty() {
                return Err(ai_error("INVALID_INPUT", "steps[].tests must not be empty"));
            }
            let mut tests = Vec::with_capacity(tests_array.len());
            for item in tests_array {
                let Some(s) = item.as_str() else {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        "steps[].tests items must be strings",
                    ));
                };
                tests.push(s.to_string());
            }
            normalize_required_string_list(tests, "steps[].tests")?
        };

        let blockers = match optional_string_array(obj, "blockers") {
            Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
            Err(resp) => return Err(resp),
        };

        steps.push(BootstrapStepInput {
            title,
            criteria,
            tests,
            blockers,
            proof_tests_mode: bm_storage::ProofMode::Off,
            proof_security_mode: bm_storage::ProofMode::Off,
            proof_perf_mode: bm_storage::ProofMode::Off,
            proof_docs_mode: bm_storage::ProofMode::Off,
        });
    }

    Ok(steps)
}
