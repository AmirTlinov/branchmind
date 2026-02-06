#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Safety, SchemaSource, Stability, Tier,
    ToolName,
};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    for def in crate::handlers::handler_definitions() {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let (cmd, op_aliases) = match vcs_cmd_for_handler_name(name) {
            Some(v) => v,
            None => continue,
        };

        let tier = match name {
            "branch_create" | "branch_list" | "checkout" | "log" | "reflog" | "show" | "diff" => {
                Tier::Gold
            }
            _ => Tier::Advanced,
        };

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::VcsOps,
            tier,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: matches!(name, "reset" | "branch_delete" | "tag_delete"),
                confirm_level: if matches!(name, "reset" | "branch_delete" | "tag_delete") {
                    ConfirmLevel::Soft
                } else {
                    ConfirmLevel::None
                },
                idempotent: matches!(name, "branch_list" | "log" | "reflog" | "show" | "diff"),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases,
            handler_name: Some(name.to_string()),
            handler: None,
        });
    }
}

fn vcs_cmd_for_handler_name(name: &str) -> Option<(String, Vec<String>)> {
    let mut op_aliases = Vec::<String>::new();
    let cmd = match name {
        "branch_create" => {
            op_aliases.push("branch.create".to_string());
            "vcs.branch.create".to_string()
        }
        "branch_list" => "vcs.branch.list".to_string(),
        "branch_rename" => "vcs.branch.rename".to_string(),
        "branch_delete" => "vcs.branch.delete".to_string(),
        "checkout" => "vcs.checkout".to_string(),
        "commit" => "vcs.commit".to_string(),
        "notes_commit" => "vcs.notes.commit".to_string(),
        "log" => "vcs.log".to_string(),
        "reflog" => "vcs.reflog".to_string(),
        "reset" => "vcs.reset".to_string(),
        "tag_create" => "vcs.tag.create".to_string(),
        "tag_list" => "vcs.tag.list".to_string(),
        "tag_delete" => "vcs.tag.delete".to_string(),
        _ => return None,
    };
    Some((cmd, op_aliases))
}
