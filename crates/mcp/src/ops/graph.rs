#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName, name_to_cmd_segments,
};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !name.starts_with("graph_") {
            continue;
        }
        let suffix = &name["graph_".len()..];
        let cmd = format!("graph.{}", name_to_cmd_segments(suffix));

        let mut op_aliases = Vec::<String>::new();
        if matches!(suffix, "query" | "apply" | "merge") {
            op_aliases.push(suffix.to_string());
        }

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::GraphOps,
            tier: Tier::Advanced,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: suffix == "apply" || suffix.contains("conflict_resolve"),
                confirm_level: if suffix == "apply" || suffix.contains("conflict_resolve") {
                    ConfirmLevel::Soft
                } else {
                    ConfirmLevel::None
                },
                idempotent: suffix == "query",
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases,
            handler_name: Some(name.to_string()),
            handler: None,
        });
    }
}
