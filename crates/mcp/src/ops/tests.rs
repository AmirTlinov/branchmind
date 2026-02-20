#![forbid(unsafe_code)]

use super::*;

#[test]
fn cmd_and_op_normalization_is_deterministic() {
    assert_eq!(normalize_cmd("tasks.snapshot").unwrap(), "tasks.snapshot");
    assert!(normalize_cmd("snapshot").is_err(), "cmd must contain '.'");

    assert_eq!(normalize_op("call").unwrap(), "call");
    assert_eq!(normalize_op("plan.create").unwrap(), "plan.create");
    assert_eq!(normalize_op("Bad").unwrap(), "bad");
}

#[test]
fn registry_has_doc_and_schema_for_all_cmds() {
    let registry = CommandRegistry::global();
    let cmds = registry.list_cmds();
    assert!(!cmds.is_empty(), "registry must expose at least one cmd");

    for spec in registry.specs() {
        assert!(!spec.cmd.trim().is_empty(), "cmd must be non-empty");
        assert!(
            super::docs_guard::doc_ref_exists(&spec.doc_ref),
            "doc_ref missing for {}",
            spec.cmd
        );
        match &spec.schema {
            SchemaSource::Handler => {
                assert!(
                    spec.handler_name.is_some(),
                    "handler schema requires handler_name for {}",
                    spec.cmd
                );
            }
            SchemaSource::Custom { args_schema, .. } => {
                assert!(
                    args_schema.is_object(),
                    "custom schema must be object for {}",
                    spec.cmd
                );
            }
        }

        let _ = spec.tier;
        let _ = spec.stability;
        let _ = spec.safety;
    }
}

#[test]
fn docs_transcripts_hooks_keep_handler_name_for_recovery_routing() {
    let registry = CommandRegistry::global();
    let open = registry
        .find_by_cmd("docs.transcripts.open")
        .expect("docs.transcripts.open must be registered");
    assert_eq!(open.handler_name.as_deref(), Some("transcripts_open"));
    assert!(
        open.handler.is_some(),
        "hooked docs command must keep custom handler"
    );

    let digest = registry
        .find_by_cmd("docs.transcripts.digest")
        .expect("docs.transcripts.digest must be registered");
    assert_eq!(digest.handler_name.as_deref(), Some("transcripts_digest"));
    assert!(
        digest.handler.is_some(),
        "hooked docs command must keep custom handler"
    );
}

#[test]
fn action_priority_ranking_is_stable() {
    assert!(ActionPriority::High.rank() < ActionPriority::Medium.rank());
    assert!(ActionPriority::Medium.rank() < ActionPriority::Low.rank());
}

#[test]
fn toolname_as_str_is_stable() {
    let names = [
        ToolName::Status,
        ToolName::Open,
        ToolName::WorkspaceOps,
        ToolName::TasksOps,
        ToolName::JobsOps,
        ToolName::ThinkOps,
        ToolName::GraphOps,
        ToolName::VcsOps,
        ToolName::DocsOps,
        ToolName::SystemOps,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        seen.insert(name.as_str());
    }
    assert_eq!(seen.len(), 10, "tool names must be unique");
}

#[test]
fn confirm_levels_cover_all_variants() {
    let _ = ConfirmLevel::None;
    let _ = ConfirmLevel::Soft;
    let _ = ConfirmLevel::Hard;
}
