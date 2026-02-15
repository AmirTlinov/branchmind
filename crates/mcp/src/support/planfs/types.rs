#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub(crate) const MIN_SLICE_TASKS: usize = 3;
pub(crate) const MAX_SLICE_TASKS: usize = 10;
pub(crate) const MIN_STEP_LIST_LEN: usize = 3;
pub(crate) const MAX_STEP_LIST_LEN: usize = 10;

#[derive(Clone, Debug)]
pub(crate) struct PlanFsReadLimits {
    pub max_file_bytes: usize,
    pub max_slices: usize,
    pub max_items_per_list: usize,
}

impl Default for PlanFsReadLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 240_000,
            max_slices: 128,
            max_items_per_list: 40,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSliceRef {
    pub id: String,
    pub title: String,
    pub file: String,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsPlanYamlHeader {
    pub plan_slug: String,
    pub title: String,
    pub objective: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub policy: String,
    pub slices: Vec<PlanFsSliceYamlRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSliceYamlRef {
    pub id: String,
    pub title: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsPlanYaml {
    pub planfs_v1: PlanFsPlanYamlHeader,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSectionBundle {
    pub goal: String,
    pub scope: Vec<String>,
    pub non_goals: Vec<String>,
    pub interfaces: Vec<String>,
    pub contracts: Vec<String>,
    pub tests: Vec<String>,
    pub proof: Vec<String>,
    pub rollback: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsBudgets {
    pub max_files: usize,
    pub max_diff_lines: usize,
    pub max_context_refs: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsDod {
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub rollback: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsStep {
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub rollback: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsTask {
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub rollback: Vec<String>,
    pub steps: Vec<PlanFsStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSliceYamlHeader {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: Option<String>,
    pub budgets: PlanFsBudgets,
    pub dod: PlanFsDod,
    pub tasks: Vec<PlanFsTask>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSliceYaml {
    pub planfs_v1: PlanFsSliceYamlHeader,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsSlice {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: Option<String>,
    pub budgets: PlanFsBudgets,
    pub dod: PlanFsDod,
    pub tasks: Vec<PlanFsTask>,
    pub sections: PlanFsSectionBundle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlanFsPlan {
    pub plan_slug: String,
    pub title: String,
    pub objective: String,
    pub constraints: Vec<String>,
    pub policy: String,
    pub slices: Vec<PlanFsSliceRef>,
    pub sections: PlanFsSectionBundle,
}
