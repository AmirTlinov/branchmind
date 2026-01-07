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
