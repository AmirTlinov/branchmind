#![forbid(unsafe_code)]

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
    ProjectGuardMismatch {
        expected: String,
        stored: String,
    },
    RevisionMismatch {
        expected: i64,
        actual: i64,
    },
    UnknownId,
    JobNotClaimable {
        job_id: String,
        status: String,
    },
    JobNotRunning {
        job_id: String,
        status: String,
    },
    JobNotCancelable {
        job_id: String,
        status: String,
    },
    JobClaimMismatch {
        job_id: String,
        expected_runner_id: Option<String>,
        actual_runner_id: String,
        expected_revision: i64,
        actual_revision: i64,
    },
    JobNotMessageable {
        job_id: String,
        status: String,
    },
    JobAlreadyTerminal {
        job_id: String,
        status: String,
    },
    JobNotRequeueable {
        job_id: String,
        status: String,
    },
    UnknownBranch,
    UnknownConflict,
    ConflictAlreadyResolved,
    MergeNotSupported,
    BranchAlreadyExists,
    BranchCycle,
    BranchDepthExceeded,
    StepNotFound,
    CheckpointsNotConfirmed {
        criteria: bool,
        tests: bool,
        security: bool,
        perf: bool,
        docs: bool,
    },
    ProofMissing {
        tests: bool,
        security: bool,
        perf: bool,
        docs: bool,
    },
    StepLeaseHeld {
        step_id: String,
        holder_agent_id: String,
        now_seq: i64,
        expires_seq: i64,
    },
    StepLeaseNotHeld {
        step_id: String,
        holder_agent_id: Option<String>,
    },
}

impl StoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::Sql(_) => "INTERNAL",
            Self::InvalidInput(message) if message.starts_with("RESET_REQUIRED") => {
                "RESET_REQUIRED"
            }
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::ProjectGuardMismatch { .. } => "PROJECT_GUARD_MISMATCH",
            Self::RevisionMismatch { .. } => "REVISION_MISMATCH",
            Self::UnknownId => "NOT_FOUND",
            Self::JobNotClaimable { .. } => "JOB_NOT_CLAIMABLE",
            Self::JobNotRunning { .. } => "JOB_NOT_RUNNING",
            Self::JobNotCancelable { .. } => "JOB_NOT_CANCELABLE",
            Self::JobClaimMismatch { .. } => "JOB_CLAIM_MISMATCH",
            Self::JobNotMessageable { .. } => "JOB_NOT_MESSAGEABLE",
            Self::JobAlreadyTerminal { .. } => "JOB_ALREADY_TERMINAL",
            Self::JobNotRequeueable { .. } => "JOB_NOT_REQUEUEABLE",
            Self::UnknownBranch => "UNKNOWN_BRANCH",
            Self::UnknownConflict => "UNKNOWN_CONFLICT",
            Self::ConflictAlreadyResolved => "CONFLICT_ALREADY_RESOLVED",
            Self::MergeNotSupported => "MERGE_NOT_SUPPORTED",
            Self::BranchAlreadyExists => "ALREADY_EXISTS",
            Self::BranchCycle => "BRANCH_CYCLE",
            Self::BranchDepthExceeded => "BRANCH_DEPTH_EXCEEDED",
            Self::StepNotFound => "STEP_NOT_FOUND",
            Self::CheckpointsNotConfirmed { .. } => "CHECKPOINTS_NOT_CONFIRMED",
            Self::ProofMissing { .. } => "PROOF_MISSING",
            Self::StepLeaseHeld { .. } => "STEP_LEASE_HELD",
            Self::StepLeaseNotHeld { .. } => "STEP_LEASE_NOT_HELD",
        }
    }

    pub fn recovery_hint(&self) -> Option<&'static str> {
        match self {
            Self::InvalidInput(message) if message.starts_with("RESET_REQUIRED") => {
                Some("legacy storage is not supported: backup data, wipe storage dir, then re-open")
            }
            Self::BranchAlreadyExists => {
                Some("use a different identifier or delete existing record")
            }
            Self::UnknownId => Some("create required entity before retry"),
            _ => None,
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::ProjectGuardMismatch { expected, stored } => write!(
                f,
                "project guard mismatch (expected={expected}, stored={stored})"
            ),
            Self::RevisionMismatch { expected, actual } => {
                write!(
                    f,
                    "revision mismatch (expected={expected}, actual={actual})"
                )
            }
            Self::UnknownId => write!(f, "unknown id"),
            Self::JobNotClaimable { job_id, status } => {
                write!(f, "job not claimable (job_id={job_id}, status={status})")
            }
            Self::JobNotRunning { job_id, status } => {
                write!(f, "job not running (job_id={job_id}, status={status})")
            }
            Self::JobNotCancelable { job_id, status } => {
                write!(f, "job not cancelable (job_id={job_id}, status={status})")
            }
            Self::JobClaimMismatch {
                job_id,
                expected_runner_id,
                actual_runner_id,
                expected_revision,
                actual_revision,
            } => match expected_runner_id {
                Some(expected_runner_id) => write!(
                    f,
                    "job claim mismatch (job_id={job_id}, expected_runner_id={expected_runner_id}, actual_runner_id={actual_runner_id}, expected_revision={expected_revision}, actual_revision={actual_revision})"
                ),
                None => write!(
                    f,
                    "job claim mismatch (job_id={job_id}, expected_runner_id=<none>, actual_runner_id={actual_runner_id}, expected_revision={expected_revision}, actual_revision={actual_revision})"
                ),
            },
            Self::JobNotMessageable { job_id, status } => {
                write!(f, "job not messageable (job_id={job_id}, status={status})")
            }
            Self::JobAlreadyTerminal { job_id, status } => {
                write!(f, "job already terminal (job_id={job_id}, status={status})")
            }
            Self::JobNotRequeueable { job_id, status } => {
                write!(f, "job not requeueable (job_id={job_id}, status={status})")
            }
            Self::UnknownBranch => write!(f, "unknown branch"),
            Self::UnknownConflict => write!(f, "unknown conflict"),
            Self::ConflictAlreadyResolved => write!(f, "conflict already resolved"),
            Self::MergeNotSupported => write!(f, "merge not supported"),
            Self::BranchAlreadyExists => write!(f, "branch already exists"),
            Self::BranchCycle => write!(f, "branch base cycle"),
            Self::BranchDepthExceeded => write!(f, "branch base depth exceeded"),
            Self::StepNotFound => write!(f, "step not found"),
            Self::CheckpointsNotConfirmed {
                criteria,
                tests,
                security,
                perf,
                docs,
            } => write!(
                f,
                "checkpoints not confirmed (criteria={criteria}, tests={tests}, security={security}, perf={perf}, docs={docs})"
            ),
            Self::ProofMissing {
                tests,
                security,
                perf,
                docs,
            } => write!(
                f,
                "proof missing (tests={tests}, security={security}, perf={perf}, docs={docs})"
            ),
            Self::StepLeaseHeld {
                step_id,
                holder_agent_id,
                now_seq,
                expires_seq,
            } => write!(
                f,
                "step lease held (step_id={step_id}, holder={holder_agent_id}, now_seq={now_seq}, expires_seq={expires_seq})"
            ),
            Self::StepLeaseNotHeld {
                step_id,
                holder_agent_id,
            } => match holder_agent_id {
                Some(holder) => write!(
                    f,
                    "step lease not held (step_id={step_id}, holder={holder})"
                ),
                None => write!(
                    f,
                    "step lease not held (step_id={step_id}, no active lease)"
                ),
            },
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}
