#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct PlanRow {
    pub id: String,
    pub revision: i64,
    pub title: String,
    pub contract: Option<String>,
    pub contract_json: Option<String>,
    pub description: Option<String>,
    pub context: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub plan_doc: Option<String>,
    pub plan_current: i64,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub criteria_auto_confirmed: bool,
    pub tests_auto_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlanChecklist {
    pub doc: Option<String>,
    pub current: i64,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PlanEditRequest {
    pub id: String,
    pub expected_revision: Option<i64>,
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub contract: Option<Option<String>>,
    pub contract_json: Option<Option<String>>,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct SetPlanStatusRequest {
    pub id: String,
    pub expected_revision: Option<i64>,
    pub status: String,
    pub status_manual: bool,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct PlanChecklistUpdateRequest {
    pub plan_id: String,
    pub expected_revision: Option<i64>,
    pub steps: Option<Vec<String>>,
    pub current: Option<i64>,
    pub doc: Option<String>,
    pub advance: bool,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct PlansSearchRequest {
    pub text: String,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct PlanSearchHit {
    pub id: String,
    pub title: String,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlansSearchResult {
    pub plans: Vec<PlanSearchHit>,
    pub has_more: bool,
}
