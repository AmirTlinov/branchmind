#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ToolName {
    Status,
    Open,
    WorkspaceOps,
    TasksOps,
    JobsOps,
    ThinkOps,
    GraphOps,
    VcsOps,
    DocsOps,
    SystemOps,
}

impl ToolName {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ToolName::Status => "status",
            ToolName::Open => "open",
            ToolName::WorkspaceOps => "workspace",
            ToolName::TasksOps => "tasks",
            ToolName::JobsOps => "jobs",
            ToolName::ThinkOps => "think",
            ToolName::GraphOps => "graph",
            ToolName::VcsOps => "vcs",
            ToolName::DocsOps => "docs",
            ToolName::SystemOps => "system",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tier {
    Gold,
    Advanced,
    Internal,
}

impl Tier {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Tier::Gold => "gold",
            Tier::Advanced => "advanced",
            Tier::Internal => "internal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stability {
    Stable,
    Experimental,
}

impl Stability {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Stability::Stable => "stable",
            Stability::Experimental => "experimental",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfirmLevel {
    None,
    Soft,
    Hard,
}

impl ConfirmLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ConfirmLevel::None => "none",
            ConfirmLevel::Soft => "soft",
            ConfirmLevel::Hard => "hard",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocRef {
    pub(crate) path: String,
    pub(crate) anchor: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Safety {
    pub(crate) destructive: bool,
    pub(crate) confirm_level: ConfirmLevel,
    pub(crate) idempotent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BudgetProfile {
    Portal,
    Default,
    Audit,
}

impl BudgetProfile {
    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "portal" => Some(Self::Portal),
            "default" => Some(Self::Default),
            "audit" => Some(Self::Audit),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Default => "default",
            Self::Audit => "audit",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BudgetCaps {
    pub(crate) max_chars: Option<usize>,
    pub(crate) context_budget: Option<usize>,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BudgetPolicy {
    pub(crate) default_profile: BudgetProfile,
    pub(crate) portal_caps: BudgetCaps,
    pub(crate) default_caps: BudgetCaps,
    pub(crate) audit_caps: BudgetCaps,
}

impl BudgetPolicy {
    pub(crate) fn caps_for(self, profile: BudgetProfile) -> BudgetCaps {
        match profile {
            BudgetProfile::Portal => self.portal_caps,
            BudgetProfile::Default => self.default_caps,
            BudgetProfile::Audit => self.audit_caps,
        }
    }

    pub(crate) fn standard() -> Self {
        Self {
            default_profile: BudgetProfile::Default,
            portal_caps: BudgetCaps {
                max_chars: Some(6_000),
                context_budget: Some(6_000),
                limit: Some(50),
            },
            default_caps: BudgetCaps {
                max_chars: Some(20_000),
                context_budget: Some(20_000),
                limit: Some(200),
            },
            audit_caps: BudgetCaps {
                max_chars: Some(80_000),
                context_budget: Some(80_000),
                limit: Some(500),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SchemaSource {
    Handler,
    Custom {
        args_schema: serde_json::Value,
        example_minimal_args: serde_json::Value,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct CommandSpec {
    pub(crate) cmd: String,
    pub(crate) domain_tool: ToolName,
    pub(crate) tier: Tier,
    pub(crate) stability: Stability,
    pub(crate) doc_ref: DocRef,
    pub(crate) safety: Safety,
    pub(crate) budget: BudgetPolicy,
    pub(crate) schema: SchemaSource,
    pub(crate) op_aliases: Vec<String>,
    pub(crate) handler_name: Option<String>,
    pub(crate) handler: Option<CommandHandler>,
}

pub(crate) type CommandHandler =
    fn(&mut crate::McpServer, &crate::ops::Envelope) -> crate::ops::OpResponse;

pub(crate) struct CommandRegistry {
    specs: Vec<CommandSpec>,
    by_cmd: BTreeMap<String, usize>,
    by_alias: BTreeMap<(ToolName, String), usize>,
}

impl CommandRegistry {
    fn build() -> Self {
        let mut specs = Vec::new();
        super::workspace::register(&mut specs);
        super::tasks::register(&mut specs);
        super::jobs::register(&mut specs);
        super::think::register(&mut specs);
        super::graph::register(&mut specs);
        super::vcs::register(&mut specs);
        super::docs::register(&mut specs);
        super::system::register(&mut specs);

        let mut by_cmd = BTreeMap::new();
        let mut by_alias = BTreeMap::new();
        let mut alias_seen = BTreeSet::new();

        for (idx, spec) in specs.iter().enumerate() {
            by_cmd.insert(spec.cmd.clone(), idx);
            for alias in spec.op_aliases.iter() {
                let alias_norm = alias.trim().to_ascii_lowercase();
                let alias_key = (spec.domain_tool, alias_norm.clone());
                if !alias_seen.insert(alias_key.clone()) {
                    panic!("op_alias collision: {alias}");
                }
                by_alias.insert(alias_key, idx);
            }
        }

        Self {
            specs,
            by_cmd,
            by_alias,
        }
    }

    pub(crate) fn global() -> &'static CommandRegistry {
        static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();
        REGISTRY.get_or_init(CommandRegistry::build)
    }

    pub(crate) fn find_by_cmd(&self, cmd: &str) -> Option<&CommandSpec> {
        self.by_cmd.get(cmd).and_then(|idx| self.specs.get(*idx))
    }

    pub(crate) fn find_by_alias(&self, tool: ToolName, alias: &str) -> Option<&CommandSpec> {
        let key = (tool, alias.trim().to_ascii_lowercase());
        self.by_alias.get(&key).and_then(|idx| self.specs.get(*idx))
    }

    pub(crate) fn find_by_handler_name(&self, handler_name: &str) -> Option<&CommandSpec> {
        let needle = handler_name.trim();
        if needle.is_empty() {
            return None;
        }
        self.specs
            .iter()
            .find(|spec| spec.handler_name.as_deref() == Some(needle))
    }

    pub(crate) fn list_cmds(&self) -> Vec<String> {
        let mut out = self.specs.iter().map(|s| s.cmd.clone()).collect::<Vec<_>>();
        out.sort();
        out.dedup();
        out
    }

    pub(crate) fn specs(&self) -> &[CommandSpec] {
        &self.specs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_name_as_str_is_stable_for_v1_surface() {
        assert_eq!(ToolName::Status.as_str(), "status");
        assert_eq!(ToolName::Open.as_str(), "open");
        assert_eq!(ToolName::WorkspaceOps.as_str(), "workspace");
        assert_eq!(ToolName::TasksOps.as_str(), "tasks");
        assert_eq!(ToolName::JobsOps.as_str(), "jobs");
        assert_eq!(ToolName::ThinkOps.as_str(), "think");
        assert_eq!(ToolName::GraphOps.as_str(), "graph");
        assert_eq!(ToolName::VcsOps.as_str(), "vcs");
        assert_eq!(ToolName::DocsOps.as_str(), "docs");
        assert_eq!(ToolName::SystemOps.as_str(), "system");
    }

    #[test]
    fn enum_as_str_is_stable() {
        assert_eq!(Tier::Gold.as_str(), "gold");
        assert_eq!(Tier::Advanced.as_str(), "advanced");
        assert_eq!(Tier::Internal.as_str(), "internal");

        assert_eq!(Stability::Stable.as_str(), "stable");
        assert_eq!(Stability::Experimental.as_str(), "experimental");

        assert_eq!(ConfirmLevel::None.as_str(), "none");
        assert_eq!(ConfirmLevel::Soft.as_str(), "soft");
        assert_eq!(ConfirmLevel::Hard.as_str(), "hard");
    }
}
