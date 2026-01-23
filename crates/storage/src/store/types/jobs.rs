#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct JobRow {
    pub id: String,
    pub revision: i64,
    pub status: String,
    pub title: String,
    pub kind: String,
    pub priority: String,
    pub task_id: Option<String>,
    pub anchor_id: Option<String>,
    pub runner: Option<String>,
    pub claim_expires_at_ms: Option<i64>,
    pub summary: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct JobEventRow {
    pub seq: i64,
    pub job_id: String,
    pub ts_ms: i64,
    pub kind: String,
    pub message: String,
    pub percent: Option<i64>,
    pub refs: Vec<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobEventGetRequest {
    pub job_id: String,
    pub seq: i64,
}

#[derive(Clone, Debug)]
pub struct JobCreateRequest {
    pub title: String,
    pub prompt: String,
    pub kind: String,
    pub priority: String,
    pub task_id: Option<String>,
    pub anchor_id: Option<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobCreateResult {
    pub job: JobRow,
    pub created_event: JobEventRow,
}

#[derive(Clone, Debug)]
pub struct JobsListRequest {
    pub status: Option<String>,
    pub task_id: Option<String>,
    pub anchor_id: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct JobsListResult {
    pub jobs: Vec<JobRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct JobsStatusCounts {
    pub running: u64,
    pub queued: u64,
}

#[derive(Clone, Debug)]
pub struct JobsRadarRequest {
    pub status: Option<String>,
    pub task_id: Option<String>,
    pub anchor_id: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct JobRadarRow {
    pub job: JobRow,
    pub last_event: Option<JobEventRow>,
    pub last_question_seq: Option<i64>,
    pub last_manager_seq: Option<i64>,
    pub last_manager_proof_seq: Option<i64>,
    pub last_error_seq: Option<i64>,
    pub last_proof_gate_seq: Option<i64>,
    pub last_checkpoint_seq: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct JobsRadarResult {
    pub rows: Vec<JobRadarRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct JobGetRequest {
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct JobClaimRequest {
    pub id: String,
    pub runner_id: String,
    pub lease_ttl_ms: u64,
    pub allow_stale: bool,
}

#[derive(Clone, Debug)]
pub struct JobClaimResult {
    pub job: JobRow,
    pub event: JobEventRow,
}

#[derive(Clone, Debug)]
pub struct JobReportRequest {
    pub id: String,
    pub runner_id: String,
    pub claim_revision: i64,
    pub kind: String,
    pub message: String,
    pub percent: Option<i64>,
    pub refs: Vec<String>,
    pub meta_json: Option<String>,
    pub lease_ttl_ms: u64,
}

#[derive(Clone, Debug)]
pub struct JobReportResult {
    pub job: JobRow,
    pub event: JobEventRow,
}

#[derive(Clone, Debug)]
pub struct JobMessageRequest {
    pub id: String,
    pub message: String,
    pub refs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct JobMessageResult {
    pub job: JobRow,
    pub event: JobEventRow,
}

#[derive(Clone, Debug)]
pub struct JobCompleteRequest {
    pub id: String,
    pub runner_id: String,
    pub claim_revision: i64,
    pub status: String,
    pub summary: Option<String>,
    pub refs: Vec<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobCompleteResult {
    pub job: JobRow,
    pub event: JobEventRow,
}

#[derive(Clone, Debug)]
pub struct JobOpenRequest {
    pub id: String,
    pub include_prompt: bool,
    pub include_events: bool,
    pub include_meta: bool,
    pub max_events: usize,
    pub before_seq: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct JobOpenResult {
    pub job: JobRow,
    pub prompt: Option<String>,
    pub meta_json: Option<String>,
    pub events: Vec<JobEventRow>,
    pub has_more_events: bool,
}

#[derive(Clone, Debug)]
pub struct JobEventsTailRequest {
    pub id: String,
    pub after_seq: i64,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct JobEventsTailResult {
    pub job_id: String,
    pub after_seq: i64,
    pub next_after_seq: i64,
    pub events: Vec<JobEventRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct JobRequeueRequest {
    pub id: String,
    pub reason: Option<String>,
    pub refs: Vec<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobRequeueResult {
    pub job: JobRow,
    pub event: JobEventRow,
}
