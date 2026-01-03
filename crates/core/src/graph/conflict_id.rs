#![forbid(unsafe_code)]

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConflictId(String);

impl ConflictId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, ConflictIdError> {
        let value = value.into();
        validate_conflict_id(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConflictIdError {
    Empty,
    InvalidFormat,
}

impl ConflictIdError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "conflict_id must not be empty",
            Self::InvalidFormat => "conflict_id must match CONFLICT-[0-9a-f]{32}",
        }
    }
}

fn validate_conflict_id(value: &str) -> Result<(), ConflictIdError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConflictIdError::Empty);
    }
    let Some(hex) = trimmed.strip_prefix("CONFLICT-") else {
        return Err(ConflictIdError::InvalidFormat);
    };
    if hex.len() != 32 {
        return Err(ConflictIdError::InvalidFormat);
    }
    if !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(ConflictIdError::InvalidFormat);
    }
    Ok(())
}
