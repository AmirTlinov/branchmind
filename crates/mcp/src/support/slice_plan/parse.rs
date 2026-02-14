#![forbid(unsafe_code)]

use super::super::ai_error;
use super::{SliceBudgets, SliceDod, SlicePlanSpec, SliceStepSpec, SliceTaskSpec};
use serde_json::{Map, Value};
use std::collections::HashSet;

const MIN_SLICE_TASKS: usize = 3;
const MAX_SLICE_TASKS: usize = 10;
const MIN_SLICE_STEPS: usize = 3;
const MAX_SLICE_STEPS: usize = 10;

fn require_object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, Value> {
    value
        .as_object()
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field}: expected object")))
}

fn require_trimmed_string(
    obj: &Map<String, Value>,
    key: &str,
    field: &str,
) -> Result<String, Value> {
    let Some(raw) = obj.get(key) else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} is required"),
        ));
    };
    let Some(raw) = raw.as_str() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} must be a string"),
        ));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} must not be empty"),
        ));
    }
    Ok(trimmed.to_string())
}

fn optional_trimmed_string(obj: &Map<String, Value>, key: &str) -> Option<String> {
    let raw = obj.get(key)?.as_str()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_string_list(
    raw: Option<&Value>,
    field: &str,
    min_len: usize,
) -> Result<Vec<String>, Value> {
    let Some(raw) = raw else {
        return if min_len == 0 {
            Ok(Vec::new())
        } else {
            Err(ai_error("INVALID_INPUT", &format!("{field} is required")))
        };
    };
    let arr = raw
        .as_array()
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field}: expected array")))?;
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for item in arr {
        let Some(text) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field}: items must be strings"),
            ));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }
    if out.len() < min_len {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected at least {min_len} non-empty unique items"),
        ));
    }
    Ok(out)
}

fn validate_duplicate_titles(tasks: &[SliceTaskSpec]) -> Result<(), Value> {
    let mut task_seen = HashSet::<String>::new();
    for task in tasks {
        let key = task.title.to_ascii_lowercase();
        if !task_seen.insert(key) {
            return Err(ai_error(
                "INVALID_INPUT",
                "slice_plan_spec.tasks: duplicate task titles are forbidden",
            ));
        }
        let mut step_seen = HashSet::<String>::new();
        for step in &task.steps {
            let step_key = step.title.to_ascii_lowercase();
            if !step_seen.insert(step_key) {
                return Err(ai_error(
                    "INVALID_INPUT",
                    &format!(
                        "slice_plan_spec.tasks[{}].steps: duplicate step titles are forbidden",
                        task.title
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_shared_context_dedup(spec: &SlicePlanSpec) -> Result<(), Value> {
    if spec.shared_context_refs.is_empty() {
        return Ok(());
    }
    let shared = spec
        .shared_context_refs
        .iter()
        .map(|v| v.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut collisions = Vec::<String>::new();
    for value in &spec.non_goals {
        if shared.contains(&value.to_ascii_lowercase()) {
            collisions.push(value.clone());
        }
    }
    for value in spec
        .dod
        .criteria
        .iter()
        .chain(spec.dod.tests.iter())
        .chain(spec.dod.blockers.iter())
    {
        if shared.contains(&value.to_ascii_lowercase()) {
            collisions.push(value.clone());
        }
    }
    for task in &spec.tasks {
        for value in task
            .success_criteria
            .iter()
            .chain(task.tests.iter())
            .chain(task.blockers.iter())
        {
            if shared.contains(&value.to_ascii_lowercase()) {
                collisions.push(value.clone());
            }
        }
        for step in &task.steps {
            for value in step
                .success_criteria
                .iter()
                .chain(step.tests.iter())
                .chain(step.blockers.iter())
            {
                if shared.contains(&value.to_ascii_lowercase()) {
                    collisions.push(value.clone());
                }
            }
        }
    }
    if !collisions.is_empty() {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "slice_plan_spec duplicates shared_context_refs content; move duplicated text into shared_context_refs and keep only references inside tasks",
        ));
    }
    Ok(())
}

fn parse_step(
    raw: &Value,
    field: &str,
    parent_tests: &[String],
    parent_blockers: &[String],
) -> Result<SliceStepSpec, Value> {
    if let Some(title) = raw.as_str() {
        let title = title.trim();
        if title.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field}: step string must not be empty"),
            ));
        }
        return Ok(SliceStepSpec {
            title: title.to_string(),
            success_criteria: vec![format!("Step completed: {title}")],
            tests: parent_tests.to_vec(),
            blockers: parent_blockers.to_vec(),
        });
    }
    let obj = require_object(raw, field)?;
    let title = require_trimmed_string(obj, "title", field)?;
    let success_criteria = normalize_string_list(
        obj.get("success_criteria"),
        &format!("{field}.success_criteria"),
        1,
    )?;
    let tests = normalize_string_list(
        obj.get("tests"),
        &format!("{field}.tests"),
        if parent_tests.is_empty() { 1 } else { 0 },
    )?;
    let blockers = normalize_string_list(
        obj.get("blockers"),
        &format!("{field}.blockers"),
        if parent_blockers.is_empty() { 1 } else { 0 },
    )?;
    Ok(SliceStepSpec {
        title,
        success_criteria,
        tests: if tests.is_empty() {
            parent_tests.to_vec()
        } else {
            tests
        },
        blockers: if blockers.is_empty() {
            parent_blockers.to_vec()
        } else {
            blockers
        },
    })
}

pub(crate) fn parse_slice_plan_spec(raw: &Value) -> Result<SlicePlanSpec, Value> {
    let obj = require_object(raw, "slice_plan_spec")?;
    let objective = require_trimmed_string(obj, "objective", "slice_plan_spec")?;
    let title = optional_trimmed_string(obj, "title")
        .unwrap_or_else(|| format!("Slice: {}", objective.chars().take(96).collect::<String>()));
    let non_goals = normalize_string_list(obj.get("non_goals"), "slice_plan_spec.non_goals", 0)?;
    let shared_context_refs = normalize_string_list(
        obj.get("shared_context_refs"),
        "slice_plan_spec.shared_context_refs",
        0,
    )?;

    let dod_obj = obj
        .get("dod")
        .ok_or_else(|| ai_error("INVALID_INPUT", "slice_plan_spec.dod is required"))?;
    let dod_map = require_object(dod_obj, "slice_plan_spec.dod")?;
    let dod = SliceDod {
        criteria: normalize_string_list(
            dod_map.get("criteria"),
            "slice_plan_spec.dod.criteria",
            1,
        )?,
        tests: normalize_string_list(dod_map.get("tests"), "slice_plan_spec.dod.tests", 1)?,
        blockers: normalize_string_list(
            dod_map.get("blockers"),
            "slice_plan_spec.dod.blockers",
            1,
        )?,
    };
    let tasks_raw = obj
        .get("tasks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "slice_plan_spec.tasks: expected array"))?;
    if tasks_raw.len() < MIN_SLICE_TASKS || tasks_raw.len() > MAX_SLICE_TASKS {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!(
                "slice_plan_spec.tasks length must be in range {}..{}",
                MIN_SLICE_TASKS, MAX_SLICE_TASKS
            ),
        ));
    }

    let mut tasks = Vec::<SliceTaskSpec>::with_capacity(tasks_raw.len());
    for (idx, task_raw) in tasks_raw.iter().enumerate() {
        let field = format!("slice_plan_spec.tasks[{idx}]");
        let task_obj = require_object(task_raw, &field)?;
        let title = require_trimmed_string(task_obj, "title", &field)?;
        let success_criteria = normalize_string_list(
            task_obj.get("success_criteria"),
            &format!("{field}.success_criteria"),
            1,
        )?;
        let tests = normalize_string_list(task_obj.get("tests"), &format!("{field}.tests"), 1)?;
        let blockers =
            normalize_string_list(task_obj.get("blockers"), &format!("{field}.blockers"), 1)?;
        let steps_raw = task_obj
            .get("steps")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field}.steps: expected array")))?;
        if steps_raw.len() < MIN_SLICE_STEPS || steps_raw.len() > MAX_SLICE_STEPS {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}.steps length must be in range {}..{}",
                    MIN_SLICE_STEPS, MAX_SLICE_STEPS
                ),
            ));
        }
        let mut steps = Vec::<SliceStepSpec>::with_capacity(steps_raw.len());
        for (step_idx, step_raw) in steps_raw.iter().enumerate() {
            let step_field = format!("{field}.steps[{step_idx}]");
            let step = parse_step(step_raw, &step_field, &tests, &blockers)?;
            steps.push(step);
        }
        tasks.push(SliceTaskSpec {
            title,
            success_criteria,
            tests,
            blockers,
            steps,
        });
    }

    validate_duplicate_titles(&tasks)?;
    let budgets = SliceBudgets::parse(obj.get("budgets"), "slice_plan_spec.budgets")?;
    let spec = SlicePlanSpec {
        title,
        objective,
        non_goals,
        shared_context_refs,
        dod,
        tasks,
        budgets,
    };
    validate_shared_context_dedup(&spec)?;
    Ok(spec)
}

pub(crate) fn parse_slice_plan_spec_from_task_context(
    raw: Option<&str>,
) -> Result<Option<SlicePlanSpec>, Value> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Value>(raw).map_err(|_| {
        ai_error(
            "PRECONDITION_FAILED",
            "slice task context is not valid JSON slice_plan_spec",
        )
    })?;
    parse_slice_plan_spec(&parsed).map(Some)
}
