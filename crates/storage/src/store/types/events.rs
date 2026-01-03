#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct EventRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub event_type: String,
    pub payload_json: String,
}

impl EventRow {
    pub fn event_id(&self) -> String {
        format!("evt_{:016}", self.seq)
    }
}
