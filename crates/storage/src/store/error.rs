#![forbid(unsafe_code)]

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
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
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
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
