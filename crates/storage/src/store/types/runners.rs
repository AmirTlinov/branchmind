#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct RunnerLeaseRow {
    pub runner_id: String,
    pub status: String,
    pub active_job_id: Option<String>,
    pub lease_expires_at_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct RunnerLeaseGetRequest {
    pub runner_id: String,
}

#[derive(Clone, Debug)]
pub struct RunnerLeaseGetResult {
    pub lease: RunnerLeaseRow,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RunnerLeaseUpsertRequest {
    pub runner_id: String,
    pub status: String,
    pub active_job_id: Option<String>,
    pub lease_ttl_ms: u64,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RunnerLeasesListRequest {
    pub status: Option<String>, // idle|live
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct RunnerLeasesListResult {
    pub runners: Vec<RunnerLeaseRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct RunnerLeasesListOfflineRequest {
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct RunnerLeasesListOfflineResult {
    pub runners: Vec<RunnerLeaseRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct RunnerStatusSnapshot {
    pub status: String, // offline|idle|live
    pub live_count: usize,
    pub idle_count: usize,
    pub offline_count: usize,
    pub runner_id: Option<String>,
    pub active_job_id: Option<String>,
    pub lease_expires_at_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct RunnerLeaseSelfHealResult {
    pub inspected: usize,
    pub cleared: usize,
}
