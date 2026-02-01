#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName, legacy_to_cmd_segments,
};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    for def in crate::tools::tool_definitions(crate::Toolset::Full) {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };

        if name == "tasks_runner_heartbeat" {
            specs.push(CommandSpec {
                cmd: "jobs.runner.heartbeat".to_string(),
                domain_tool: ToolName::JobsOps,
                tier: Tier::Advanced,
                stability: Stability::Stable,
                doc_ref: DocRef {
                    path: "docs/contracts/V1_COMMANDS.md".to_string(),
                    anchor: "#jobs.runner.heartbeat".to_string(),
                },
                safety: Safety {
                    destructive: false,
                    confirm_level: ConfirmLevel::None,
                    idempotent: true,
                },
                budget: BudgetPolicy::standard(),
                schema: SchemaSource::Legacy,
                op_aliases: Vec::new(),
                legacy_tool: Some(name.to_string()),
                handler: None,
            });
            continue;
        }

        if !name.starts_with("tasks_jobs_") {
            continue;
        }
        let suffix = &name["tasks_jobs_".len()..];
        let cmd = format!("jobs.{}", legacy_to_cmd_segments(suffix));

        let mut op_aliases = Vec::<String>::new();
        if matches!(suffix, "create" | "list" | "radar" | "open") {
            op_aliases.push(suffix.to_string());
        }

        let doc_ref_anchor = if matches!(suffix, "create" | "list" | "radar" | "open") {
            format!("#{cmd}")
        } else {
            "#cmd-index".to_string()
        };
        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::JobsOps,
            tier: Tier::Advanced,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: doc_ref_anchor,
            },
            safety: Safety {
                destructive: matches!(suffix, "complete" | "requeue"),
                confirm_level: if matches!(suffix, "complete" | "requeue") {
                    ConfirmLevel::Soft
                } else {
                    ConfirmLevel::None
                },
                idempotent: matches!(suffix, "list" | "radar" | "open" | "tail"),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Legacy,
            op_aliases,
            legacy_tool: Some(name.to_string()),
            handler: None,
        });
    }
}
