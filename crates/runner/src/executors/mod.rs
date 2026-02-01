#![forbid(unsafe_code)]

pub(crate) mod claude_code;
pub(crate) mod codex;
pub(crate) mod output_schema;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutorKind {
    Codex,
    ClaudeCode,
}

impl ExecutorKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ExecutorKind::Codex => "codex",
            ExecutorKind::ClaudeCode => "claude_code",
        }
    }
}
