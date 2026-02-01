#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, DocRef, Safety, SchemaSource, Stability, Tier, ToolName,
};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    specs.push(CommandSpec {
        cmd: "workspace.use".to_string(),
        domain_tool: ToolName::WorkspaceOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#workspace.use".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: crate::ops::ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Legacy,
        op_aliases: vec!["use".to_string()],
        legacy_tool: Some("workspace_use".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "workspace.reset".to_string(),
        domain_tool: ToolName::WorkspaceOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#workspace.reset".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: crate::ops::ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Legacy,
        op_aliases: vec!["reset".to_string()],
        legacy_tool: Some("workspace_reset".to_string()),
        handler: None,
    });
}
