#![forbid(unsafe_code)]

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
    UnknownId,
    UnknownBranch,
    BranchAlreadyExists,
    BranchCycle,
    BranchDepthExceeded,
}

impl StoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::Sql(_) => "INTERNAL",
            Self::InvalidInput(message) if message.starts_with("RESET_REQUIRED") => {
                "RESET_REQUIRED"
            }
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::UnknownId | Self::UnknownBranch => "NOT_FOUND",
            Self::BranchAlreadyExists => "ALREADY_EXISTS",
            Self::BranchCycle => "BRANCH_CYCLE",
            Self::BranchDepthExceeded => "BRANCH_DEPTH_EXCEEDED",
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
            Self::UnknownId | Self::UnknownBranch => Some("create required entity before retry"),
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
            Self::UnknownId => write!(f, "unknown id"),
            Self::UnknownBranch => write!(f, "unknown branch"),
            Self::BranchAlreadyExists => write!(f, "branch already exists"),
            Self::BranchCycle => write!(f, "branch parent cycle"),
            Self::BranchDepthExceeded => write!(f, "branch depth exceeded"),
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
