#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct PlanSliceRow {
    pub plan_id: String,
    pub slice_id: String,
    pub slice_task_id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub budgets_json: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlanSliceInsertRequest {
    pub plan_id: String,
    pub slice_id: String,
    pub slice_task_id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub budgets_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlanSliceStatusUpdateRequest {
    pub plan_id: String,
    pub slice_id: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct PlanSlicesSearchRequest {
    pub text: String,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct PlanSliceSearchHit {
    pub plan_id: String,
    pub slice_id: String,
    pub slice_task_id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlanSlicesSearchResult {
    pub slices: Vec<PlanSliceSearchHit>,
    pub has_more: bool,
}
