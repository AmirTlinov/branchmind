#![forbid(unsafe_code)]

use super::events::EventRow;

#[derive(Clone, Debug)]
pub struct StepRef {
    pub step_id: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProofMode {
    #[default]
    Off,
    Warn,
    Require,
}

impl ProofMode {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Off => 0,
            Self::Warn => 1,
            Self::Require => 2,
        }
    }

    pub fn from_i64(raw: i64) -> Self {
        match raw {
            1 => Self::Warn,
            2 => Self::Require,
            _ => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Require => "require",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct StepSelector {
    pub step_id: Option<String>,
    pub path: Option<bm_core::paths::StepPath>,
}

#[derive(Clone, Debug)]
pub struct StepOpResult {
    pub task_revision: i64,
    pub step: StepRef,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct StepCloseResult {
    pub task_revision: i64,
    pub step: StepRef,
    pub events: Vec<EventRow>,
}

#[derive(Clone, Debug)]
pub struct DecomposeResult {
    pub task_revision: i64,
    pub steps: Vec<StepRef>,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct StepPatch {
    pub title: Option<String>,
    pub success_criteria: Option<Vec<String>>,
    pub tests: Option<Vec<String>>,
    pub blockers: Option<Vec<String>>,
    pub proof_tests_mode: Option<ProofMode>,
    pub proof_security_mode: Option<ProofMode>,
    pub proof_perf_mode: Option<ProofMode>,
    pub proof_docs_mode: Option<ProofMode>,
}

#[derive(Clone, Debug)]
pub struct StepDefineRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub patch: StepPatch,
}

#[derive(Clone, Debug)]
pub struct StepNoteRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct StepVerifyRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub criteria_confirmed: Option<bool>,
    pub tests_confirmed: Option<bool>,
    pub security_confirmed: Option<bool>,
    pub perf_confirmed: Option<bool>,
    pub docs_confirmed: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct StepCloseRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub criteria_confirmed: Option<bool>,
    pub tests_confirmed: Option<bool>,
    pub security_confirmed: Option<bool>,
    pub perf_confirmed: Option<bool>,
    pub docs_confirmed: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct StepProgressRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub completed: bool,
    pub force: bool,
    pub record_undo: bool,
}

#[derive(Clone, Debug)]
pub struct StepBlockSetRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub blocked: bool,
    pub reason: Option<String>,
    pub record_undo: bool,
}

#[derive(Clone, Debug)]
pub struct StepPatchRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub patch: StepPatch,
    pub event_payload_json: String,
    pub record_undo: bool,
}

#[derive(Clone, Debug)]
pub struct NewStep {
    pub title: String,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StepStatus {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub require_security: bool,
    pub require_perf: bool,
    pub require_docs: bool,
    pub proof_tests_mode: ProofMode,
    pub proof_security_mode: ProofMode,
    pub proof_perf_mode: ProofMode,
    pub proof_docs_mode: ProofMode,
    pub proof_tests_present: bool,
    pub proof_security_present: bool,
    pub proof_perf_present: bool,
    pub proof_docs_present: bool,
    pub completed: bool,
}

#[derive(Clone, Debug)]
pub struct TaskStepSummary {
    pub total_steps: i64,
    pub completed_steps: i64,
    pub open_steps: i64,
    pub missing_criteria: i64,
    pub missing_tests: i64,
    pub missing_security: i64,
    pub missing_perf: i64,
    pub missing_docs: i64,
    pub missing_proof_tests: i64,
    pub missing_proof_security: i64,
    pub missing_proof_perf: i64,
    pub missing_proof_docs: i64,
    pub first_open: Option<StepStatus>,
}

#[derive(Clone, Debug)]
pub struct StepDetail {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub completed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StepListRow {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub completed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StepLease {
    pub step_id: String,
    pub holder_agent_id: String,
    pub acquired_seq: i64,
    pub expires_seq: i64,
}

#[derive(Clone, Debug)]
pub struct StepLeaseGetRequest {
    pub task_id: String,
    pub selector: StepSelector,
}

#[derive(Clone, Debug)]
pub struct StepLeaseClaimRequest {
    pub task_id: String,
    pub selector: StepSelector,
    pub agent_id: String,
    pub ttl_seq: i64,
    pub force: bool,
}

#[derive(Clone, Debug)]
pub struct StepLeaseRenewRequest {
    pub task_id: String,
    pub selector: StepSelector,
    pub agent_id: String,
    pub ttl_seq: i64,
}

#[derive(Clone, Debug)]
pub struct StepLeaseReleaseRequest {
    pub task_id: String,
    pub selector: StepSelector,
    pub agent_id: String,
}

#[derive(Clone, Debug)]
pub struct StepLeaseGetResult {
    pub step: StepRef,
    pub lease: Option<StepLease>,
    pub now_seq: i64,
}

#[derive(Clone, Debug)]
pub struct StepLeaseOpResult {
    pub step: StepRef,
    pub lease: Option<StepLease>,
    pub event: Option<EventRow>,
    pub now_seq: i64,
}
