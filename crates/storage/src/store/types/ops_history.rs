#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct OpsHistoryRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub intent: String,
    pub payload_json: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub undoable: bool,
    pub undone: bool,
}
