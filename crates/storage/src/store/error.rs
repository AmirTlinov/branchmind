#![forbid(unsafe_code)]

use bm_core::DomainError;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvariantViolation(DomainError),
    InvalidInput(&'static str),
    NotFound {
        entity: &'static str,
        id: String,
    },
    AlreadyExists {
        entity: &'static str,
        id: String,
    },
    ResetRequired {
        expected_schema: i64,
        found_schema: Option<i64>,
        reason: String,
    },
}

impl StoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::Sql(_) => "INTERNAL",
            Self::InvariantViolation(_) => "INVARIANT_VIOLATION",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::AlreadyExists { .. } => "ALREADY_EXISTS",
            Self::ResetRequired { .. } => "RESET_REQUIRED",
        }
    }

    pub fn recovery_hint(&self) -> Option<&'static str> {
        match self {
            Self::ResetRequired { .. } => {
                Some("legacy storage is not supported: backup data, wipe storage dir, then re-open")
            }
            Self::AlreadyExists { .. } => {
                Some("use a different identifier or delete existing record")
            }
            Self::NotFound { .. } => Some("create the required entity first"),
            Self::InvariantViolation(_) => Some("fix field constraints before retry"),
            Self::InvalidInput(_) => Some("check request payload"),
            Self::Io(_) | Self::Sql(_) => None,
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvariantViolation(err) => write!(f, "domain invariant: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::NotFound { entity, id } => write!(f, "{entity} not found: {id}"),
            Self::AlreadyExists { entity, id } => write!(f, "{entity} already exists: {id}"),
            Self::ResetRequired {
                expected_schema,
                found_schema,
                reason,
            } => match found_schema {
                Some(found_schema) => write!(
                    f,
                    "storage reset required (expected_schema={expected_schema}, found_schema={found_schema}): {reason}"
                ),
                None => write!(
                    f,
                    "storage reset required (expected_schema={expected_schema}, found_schema=<unknown>): {reason}"
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

impl From<DomainError> for StoreError {
    fn from(value: DomainError) -> Self {
        Self::InvariantViolation(value)
    }
}
