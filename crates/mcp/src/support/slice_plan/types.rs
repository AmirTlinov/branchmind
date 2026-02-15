#![forbid(unsafe_code)]

use super::super::ai_error;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct SliceBudgets {
    pub max_context_refs: usize,
    pub max_files: usize,
    pub max_diff_lines: usize,
}

impl Default for SliceBudgets {
    fn default() -> Self {
        Self {
            max_context_refs: 24,
            max_files: 12,
            max_diff_lines: 1200,
        }
    }
}

impl SliceBudgets {
    pub(crate) fn parse(raw: Option<&Value>, field: &str) -> Result<SliceBudgets, Value> {
        let Some(raw) = raw else {
            return Ok(SliceBudgets::default());
        };
        let Some(map) = raw.as_object() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field} must be an object"),
            ));
        };
        let parse_usize =
            |key: &str, default: usize, min: usize, max: usize| -> Result<usize, Value> {
                let Some(raw) = map.get(key) else {
                    return Ok(default);
                };
                let Some(v) = raw.as_u64() else {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        &format!("{field}.{key} must be integer"),
                    ));
                };
                Ok((v as usize).clamp(min, max))
            };
        Ok(SliceBudgets {
            max_context_refs: parse_usize("max_context_refs", 24, 8, 64)?,
            max_files: parse_usize("max_files", 12, 1, 64)?,
            max_diff_lines: parse_usize("max_diff_lines", 1200, 50, 20_000)?,
        })
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "max_context_refs": self.max_context_refs,
            "max_files": self.max_files,
            "max_diff_lines": self.max_diff_lines
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SliceDod {
    pub criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
}

impl SliceDod {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "criteria": self.criteria,
            "tests": self.tests,
            "blockers": self.blockers
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SliceStepSpec {
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
}

impl SliceStepSpec {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "success_criteria": self.success_criteria,
            "tests": self.tests,
            "blockers": self.blockers
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SliceTaskSpec {
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub steps: Vec<SliceStepSpec>,
}

impl SliceTaskSpec {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "success_criteria": self.success_criteria,
            "tests": self.tests,
            "blockers": self.blockers,
            "steps": self.steps.iter().map(SliceStepSpec::to_json).collect::<Vec<_>>()
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SlicePlanSpec {
    pub title: String,
    pub objective: String,
    pub non_goals: Vec<String>,
    pub shared_context_refs: Vec<String>,
    pub dod: SliceDod,
    pub tasks: Vec<SliceTaskSpec>,
    pub budgets: SliceBudgets,
}

impl SlicePlanSpec {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "objective": self.objective,
            "non_goals": self.non_goals,
            "shared_context_refs": self.shared_context_refs,
            "dod": self.dod.to_json(),
            "tasks": self.tasks.iter().map(SliceTaskSpec::to_json).collect::<Vec<_>>(),
            "budgets": self.budgets.to_json()
        })
    }
}
