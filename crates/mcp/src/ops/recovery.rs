#![forbid(unsafe_code)]

pub(crate) fn removed_knowledge_recovery(cmd: &str) -> Option<&'static str> {
    let is_removed = cmd == "think.add.knowledge"
        || cmd == "think.note.promote"
        || cmd == "think.knowledge"
        || cmd.starts_with("think.knowledge.");
    if is_removed {
        Some(
            "Use repo-local skills (.agents/skills/**) and PlanFS docs; knowledge removed by design.",
        )
    } else {
        None
    }
}
