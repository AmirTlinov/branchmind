#![forbid(unsafe_code)]

use std::fmt;

pub const MAX_IDENTIFIER_LEN: usize = 128;
pub const MAX_COMMIT_MESSAGE_LEN: usize = 1_024;
pub const MAX_COMMIT_BODY_LEN: usize = 65_536;
pub const MAX_MERGE_STRATEGY_LEN: usize = 64;
pub const MAX_MERGE_SUMMARY_LEN: usize = 4_096;

/// Core domain invariant failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainError {
    EmptyField {
        field: &'static str,
    },
    FieldTooLong {
        field: &'static str,
        max_len: usize,
    },
    InvalidFirstChar {
        field: &'static str,
    },
    InvalidChar {
        field: &'static str,
        ch: char,
        index: usize,
    },
    SameValue {
        field_a: &'static str,
        field_b: &'static str,
    },
    NegativeTimestamp {
        field: &'static str,
    },
    TimestampOrder {
        earlier: &'static str,
        later: &'static str,
    },
    ContainsNul {
        field: &'static str,
    },
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField { field } => write!(f, "field '{field}' must not be empty"),
            Self::FieldTooLong { field, max_len } => {
                write!(f, "field '{field}' is too long (max={max_len})")
            }
            Self::InvalidFirstChar { field } => {
                write!(f, "field '{field}' must start with [a-z0-9A-Z]")
            }
            Self::InvalidChar { field, ch, index } => {
                write!(
                    f,
                    "field '{field}' has invalid char '{ch}' at index {index}"
                )
            }
            Self::SameValue { field_a, field_b } => {
                write!(f, "fields '{field_a}' and '{field_b}' must differ")
            }
            Self::NegativeTimestamp { field } => {
                write!(f, "field '{field}' must be >= 0")
            }
            Self::TimestampOrder { earlier, later } => {
                write!(f, "field '{later}' must be >= '{earlier}'")
            }
            Self::ContainsNul { field } => {
                write!(f, "field '{field}' must not contain NUL bytes")
            }
        }
    }
}

impl std::error::Error for DomainError {}

/// A branch in thought history.
///
/// Explicit invariants:
/// - `workspace_id`, `branch_id`, `parent_branch_id`, `head_commit_id` use the same canonical identifier form
///   (`[a-z0-9][a-z0-9._/-]{0,127}`, lowercase normalized).
/// - `parent_branch_id != branch_id`.
/// - `updated_at_ms >= created_at_ms` and both timestamps are non-negative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThoughtBranch {
    workspace_id: String,
    branch_id: String,
    parent_branch_id: Option<String>,
    head_commit_id: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl ThoughtBranch {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        workspace_id: impl Into<String>,
        branch_id: impl Into<String>,
        parent_branch_id: Option<String>,
        head_commit_id: Option<String>,
        created_at_ms: i64,
        updated_at_ms: i64,
    ) -> Result<Self, DomainError> {
        let workspace_id = normalize_identifier("workspace_id", workspace_id.into())?;
        let branch_id = normalize_identifier("branch_id", branch_id.into())?;
        let parent_branch_id = parent_branch_id
            .map(|value| normalize_identifier("parent_branch_id", value))
            .transpose()?;
        let head_commit_id = head_commit_id
            .map(|value| normalize_identifier("head_commit_id", value))
            .transpose()?;

        validate_non_negative_timestamp("created_at_ms", created_at_ms)?;
        validate_non_negative_timestamp("updated_at_ms", updated_at_ms)?;
        if updated_at_ms < created_at_ms {
            return Err(DomainError::TimestampOrder {
                earlier: "created_at_ms",
                later: "updated_at_ms",
            });
        }

        if parent_branch_id
            .as_ref()
            .is_some_and(|parent| parent == &branch_id)
        {
            return Err(DomainError::SameValue {
                field_a: "parent_branch_id",
                field_b: "branch_id",
            });
        }

        Ok(Self {
            workspace_id,
            branch_id,
            parent_branch_id,
            head_commit_id,
            created_at_ms,
            updated_at_ms,
        })
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    pub fn parent_branch_id(&self) -> Option<&str> {
        self.parent_branch_id.as_deref()
    }

    pub fn head_commit_id(&self) -> Option<&str> {
        self.head_commit_id.as_deref()
    }

    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub fn updated_at_ms(&self) -> i64 {
        self.updated_at_ms
    }
}

/// A commit in thought history.
///
/// Explicit invariants:
/// - Identifiers are canonical and lowercase normalized.
/// - `parent_commit_id != commit_id`.
/// - `message` and `body` are trimmed, non-empty, and bounded.
/// - `created_at_ms` is non-negative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThoughtCommit {
    workspace_id: String,
    branch_id: String,
    commit_id: String,
    parent_commit_id: Option<String>,
    message: String,
    body: String,
    created_at_ms: i64,
}

impl ThoughtCommit {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        workspace_id: impl Into<String>,
        branch_id: impl Into<String>,
        commit_id: impl Into<String>,
        parent_commit_id: Option<String>,
        message: impl Into<String>,
        body: impl Into<String>,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        let workspace_id = normalize_identifier("workspace_id", workspace_id.into())?;
        let branch_id = normalize_identifier("branch_id", branch_id.into())?;
        let commit_id = normalize_identifier("commit_id", commit_id.into())?;
        let parent_commit_id = parent_commit_id
            .map(|value| normalize_identifier("parent_commit_id", value))
            .transpose()?;
        let message = normalize_text("message", message.into(), MAX_COMMIT_MESSAGE_LEN)?;
        let body = normalize_text("body", body.into(), MAX_COMMIT_BODY_LEN)?;

        validate_non_negative_timestamp("created_at_ms", created_at_ms)?;

        if parent_commit_id
            .as_ref()
            .is_some_and(|parent| parent == &commit_id)
        {
            return Err(DomainError::SameValue {
                field_a: "parent_commit_id",
                field_b: "commit_id",
            });
        }

        Ok(Self {
            workspace_id,
            branch_id,
            commit_id,
            parent_commit_id,
            message,
            body,
            created_at_ms,
        })
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    pub fn commit_id(&self) -> &str {
        &self.commit_id
    }

    pub fn parent_commit_id(&self) -> Option<&str> {
        self.parent_commit_id.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }
}

/// A merge artifact that links branch integration with a synthesis commit.
///
/// Explicit invariants:
/// - Identifiers are canonical and lowercase normalized.
/// - `source_branch_id != target_branch_id`.
/// - `strategy` and `summary` are trimmed, non-empty, and bounded.
/// - `created_at_ms` is non-negative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeRecord {
    workspace_id: String,
    merge_id: String,
    source_branch_id: String,
    target_branch_id: String,
    synthesis_commit_id: String,
    strategy: String,
    summary: String,
    created_at_ms: i64,
}

impl MergeRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        workspace_id: impl Into<String>,
        merge_id: impl Into<String>,
        source_branch_id: impl Into<String>,
        target_branch_id: impl Into<String>,
        synthesis_commit_id: impl Into<String>,
        strategy: impl Into<String>,
        summary: impl Into<String>,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        let workspace_id = normalize_identifier("workspace_id", workspace_id.into())?;
        let merge_id = normalize_identifier("merge_id", merge_id.into())?;
        let source_branch_id = normalize_identifier("source_branch_id", source_branch_id.into())?;
        let target_branch_id = normalize_identifier("target_branch_id", target_branch_id.into())?;
        let synthesis_commit_id =
            normalize_identifier("synthesis_commit_id", synthesis_commit_id.into())?;
        let strategy = normalize_text("strategy", strategy.into(), MAX_MERGE_STRATEGY_LEN)?;
        let summary = normalize_text("summary", summary.into(), MAX_MERGE_SUMMARY_LEN)?;

        validate_non_negative_timestamp("created_at_ms", created_at_ms)?;

        if source_branch_id == target_branch_id {
            return Err(DomainError::SameValue {
                field_a: "source_branch_id",
                field_b: "target_branch_id",
            });
        }

        Ok(Self {
            workspace_id,
            merge_id,
            source_branch_id,
            target_branch_id,
            synthesis_commit_id,
            strategy,
            summary,
            created_at_ms,
        })
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn merge_id(&self) -> &str {
        &self.merge_id
    }

    pub fn source_branch_id(&self) -> &str {
        &self.source_branch_id
    }

    pub fn target_branch_id(&self) -> &str {
        &self.target_branch_id
    }

    pub fn synthesis_commit_id(&self) -> &str {
        &self.synthesis_commit_id
    }

    pub fn strategy(&self) -> &str {
        &self.strategy
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }
}

fn normalize_identifier(field: &'static str, value: String) -> Result<String, DomainError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(DomainError::EmptyField { field });
    }

    let len = value.chars().count();
    if len > MAX_IDENTIFIER_LEN {
        return Err(DomainError::FieldTooLong {
            field,
            max_len: MAX_IDENTIFIER_LEN,
        });
    }

    if value.contains('\0') {
        return Err(DomainError::ContainsNul { field });
    }

    let mut chars = value.chars();
    let first = chars.next().ok_or(DomainError::EmptyField { field })?;
    if !first.is_ascii_alphanumeric() {
        return Err(DomainError::InvalidFirstChar { field });
    }

    for (index, ch) in value.chars().enumerate() {
        if index == 0 {
            continue;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/') {
            continue;
        }
        return Err(DomainError::InvalidChar { field, ch, index });
    }

    Ok(value)
}

pub fn canonical_identifier(
    field: &'static str,
    value: impl Into<String>,
) -> Result<String, DomainError> {
    normalize_identifier(field, value.into())
}

fn normalize_text(
    field: &'static str,
    value: String,
    max_len: usize,
) -> Result<String, DomainError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if value.chars().count() > max_len {
        return Err(DomainError::FieldTooLong { field, max_len });
    }
    if value.contains('\0') {
        return Err(DomainError::ContainsNul { field });
    }
    Ok(value)
}

fn validate_non_negative_timestamp(field: &'static str, value: i64) -> Result<(), DomainError> {
    if value < 0 {
        return Err(DomainError::NegativeTimestamp { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_constructor_normalizes_identifiers_and_time() {
        let branch = ThoughtBranch::try_new(
            " Workspace-1 ",
            " MAIN ",
            Some(" Root ".to_string()),
            Some(" C-001 ".to_string()),
            10,
            15,
        )
        .expect("branch should be valid");

        assert_eq!(branch.workspace_id(), "workspace-1");
        assert_eq!(branch.branch_id(), "main");
        assert_eq!(branch.parent_branch_id(), Some("root"));
        assert_eq!(branch.head_commit_id(), Some("c-001"));
        assert_eq!(branch.created_at_ms(), 10);
        assert_eq!(branch.updated_at_ms(), 15);
    }

    #[test]
    fn branch_rejects_self_parent_and_invalid_ordered_time() {
        let same_parent = ThoughtBranch::try_new("ws", "main", Some("main".into()), None, 0, 0)
            .expect_err("parent=branch must fail");
        assert!(matches!(
            same_parent,
            DomainError::SameValue {
                field_a: "parent_branch_id",
                field_b: "branch_id"
            }
        ));

        let bad_time = ThoughtBranch::try_new("ws", "main", None, None, 20, 10)
            .expect_err("updated_at < created_at must fail");
        assert!(matches!(
            bad_time,
            DomainError::TimestampOrder {
                earlier: "created_at_ms",
                later: "updated_at_ms"
            }
        ));
    }

    #[test]
    fn commit_invariants_are_fail_closed() {
        let commit = ThoughtCommit::try_new(
            "ws",
            "main",
            "c-1",
            Some("c-1".into()),
            "message",
            "body",
            1,
        )
        .expect_err("same parent/commit id must fail");
        assert!(matches!(
            commit,
            DomainError::SameValue {
                field_a: "parent_commit_id",
                field_b: "commit_id"
            }
        ));

        let bad_body = ThoughtCommit::try_new("ws", "main", "c-2", None, "m", "", 1)
            .expect_err("empty body must fail");
        assert!(matches!(
            bad_body,
            DomainError::EmptyField { field: "body" }
        ));
    }

    #[test]
    fn merge_record_requires_distinct_branches() {
        let err = MergeRecord::try_new(
            "ws", "merge-1", "main", "main", "c-9", "squash", "summary", 2,
        )
        .expect_err("source and target must differ");

        assert!(matches!(
            err,
            DomainError::SameValue {
                field_a: "source_branch_id",
                field_b: "target_branch_id"
            }
        ));
    }
}
