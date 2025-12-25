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

pub mod paths {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct StepPath {
        indices: Vec<usize>,
    }

    impl StepPath {
        pub fn indices(&self) -> &[usize] {
            &self.indices
        }

        pub fn parse(value: &str) -> Result<Self, StepPathError> {
            let value = value.trim();
            if value.is_empty() {
                return Err(StepPathError::Empty);
            }

            let mut indices = Vec::new();
            for segment in value.split('.') {
                let Some(raw) = segment.strip_prefix("s:") else {
                    return Err(StepPathError::InvalidSegment);
                };
                let index = raw
                    .parse::<usize>()
                    .map_err(|_| StepPathError::InvalidIndex)?;
                indices.push(index);
            }

            if indices.is_empty() {
                return Err(StepPathError::Empty);
            }

            Ok(Self { indices })
        }

        pub fn child(&self, ordinal: usize) -> Self {
            let mut indices = self.indices.clone();
            indices.push(ordinal);
            Self { indices }
        }

        pub fn root(ordinal: usize) -> Self {
            Self {
                indices: vec![ordinal],
            }
        }

        pub fn to_string(&self) -> String {
            self.indices
                .iter()
                .map(|i| format!("s:{i}"))
                .collect::<Vec<_>>()
                .join(".")
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum StepPathError {
        Empty,
        InvalidSegment,
        InvalidIndex,
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

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ReasoningRef {
        pub branch: String,
        pub notes_doc: String,
        pub graph_doc: String,
        pub trace_doc: String,
    }

    impl ReasoningRef {
        pub fn for_entity(kind: TaskKind, id: &str) -> Self {
            let branch_prefix = match kind {
                TaskKind::Plan => "plan",
                TaskKind::Task => "task",
            };
            Self {
                branch: format!("{branch_prefix}/{id}"),
                notes_doc: "notes".to_string(),
                graph_doc: format!("{id}-graph"),
                trace_doc: format!("{id}-trace"),
            }
        }
    }
}

pub mod think {
    pub const SUPPORTED_THINK_CARD_TYPES: &[&str] = &[
        "frame",
        "hypothesis",
        "question",
        "test",
        "evidence",
        "decision",
        "note",
        "update",
    ];

    pub fn is_supported_think_card_type(value: &str) -> bool {
        let value = value.trim();
        SUPPORTED_THINK_CARD_TYPES
            .iter()
            .any(|candidate| *candidate == value)
    }
}
