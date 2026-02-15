#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct JobBusMessageRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub thread_id: String,
    pub from_agent_id: String,
    pub from_job_id: Option<String>,
    pub to_agent_id: Option<String>,
    pub kind: String,
    pub summary: String,
    pub refs: Vec<String>,
    pub payload_json: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct JobBusPublishRequest {
    pub idempotency_key: String,
    pub thread_id: String,
    pub from_agent_id: String,
    pub from_job_id: Option<String>,
    pub to_agent_id: Option<String>,
    pub kind: String,
    pub summary: String,
    pub refs: Vec<String>,
    pub payload_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobBusPublishResult {
    pub message: JobBusMessageRow,
    pub deduped: bool,
}

#[derive(Clone, Debug)]
pub struct JobBusPullRequest {
    pub consumer_id: String,
    pub thread_id: String,
    pub after_seq: Option<i64>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct JobBusPullResult {
    pub messages: Vec<JobBusMessageRow>,
    pub next_after_seq: i64,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct JobBusAckRequest {
    pub consumer_id: String,
    pub thread_id: String,
    pub after_seq: i64,
}

#[derive(Clone, Debug)]
pub struct JobBusAckResult {
    pub consumer_id: String,
    pub thread_id: String,
    pub after_seq: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadStatusRequest {
    pub consumer_id: String,
    pub thread_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadStatusRow {
    pub thread_id: String,
    pub after_seq: i64,
    pub unread_count: i64,
    pub last_seq: Option<i64>,
    pub last_ts_ms: Option<i64>,
    pub last_kind: Option<String>,
    pub last_summary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadStatusResult {
    pub rows: Vec<JobBusThreadStatusRow>,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadsRecentRequest {
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadsRecentRow {
    pub thread_id: String,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct JobBusThreadsRecentResult {
    pub rows: Vec<JobBusThreadsRecentRow>,
}
