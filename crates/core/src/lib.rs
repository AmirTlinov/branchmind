#![forbid(unsafe_code)]

pub mod ids {
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct WorkspaceId(String);

    impl WorkspaceId {
        pub fn as_str(&self) -> &str {
            &self.0
        }

        pub fn try_new(value: impl Into<String>) -> Result<Self, WorkspaceIdError> {
            let value = value.into();
            validate_workspace_id(&value)?;
            Ok(Self(value))
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum WorkspaceIdError {
        Empty,
        TooLong,
        InvalidFirstChar,
        InvalidChar { ch: char, index: usize },
    }

    fn validate_workspace_id(value: &str) -> Result<(), WorkspaceIdError> {
        if value.is_empty() {
            return Err(WorkspaceIdError::Empty);
        }
        if value.len() > 128 {
            return Err(WorkspaceIdError::TooLong);
        }
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(WorkspaceIdError::Empty);
        };
        if !first.is_ascii_alphanumeric() {
            return Err(WorkspaceIdError::InvalidFirstChar);
        }
        for (index, ch) in value.chars().enumerate() {
            if index == 0 {
                continue;
            }
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-') {
                continue;
            }
            return Err(WorkspaceIdError::InvalidChar { ch, index });
        }
        Ok(())
    }
}

pub mod model {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TaskKind {
        Plan,
        Task,
    }

    impl TaskKind {
        pub fn as_str(self) -> &'static str {
            match self {
                TaskKind::Plan => "plan",
                TaskKind::Task => "task",
            }
        }
    }
}
