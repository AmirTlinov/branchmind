#![forbid(unsafe_code)]

use super::events::EventRow;
use super::steps::{StepRef, StepSelector};

#[derive(Clone, Debug)]
pub struct EvidenceArtifactInput {
    pub kind: String,
    pub command: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i64>,
    pub diff: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub external_uri: Option<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EvidenceCaptureRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub agent_id: Option<String>,
    pub selector: StepSelector,
    pub artifacts: Vec<EvidenceArtifactInput>,
    pub checks: Vec<String>,
    pub attachments: Vec<String>,
    pub checkpoints: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EvidenceCaptureResult {
    pub revision: i64,
    pub step: Option<StepRef>,
    pub event: EventRow,
}
