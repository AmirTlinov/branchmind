#![forbid(unsafe_code)]

#[derive(Clone, Copy)]
pub(crate) struct NoteTemplateInfo {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) fn built_in_note_templates() -> Vec<NoteTemplateInfo> {
    vec![
        NoteTemplateInfo {
            id: "initiative",
            title: "Initiative seed note",
            description: "Principal-grade initiative capsule for goals, constraints, risks, and next actions.",
        },
        NoteTemplateInfo {
            id: "decision",
            title: "Decision record",
            description: "Small decision capsule: context, options, decision, consequences.",
        },
    ]
}

fn render_initiative(goal: Option<&str>) -> String {
    let goal = goal.unwrap_or("").trim();
    let goal_line = if goal.is_empty() {
        "<fill goal>".to_string()
    } else {
        goal.to_string()
    };

    [
        "# Goal",
        &goal_line,
        "",
        "# Scope / Non-goals",
        "- Scope:",
        "- Non-goals:",
        "",
        "# Constraints",
        "- Time:",
        "- Safety:",
        "- Performance:",
        "- Compatibility:",
        "",
        "# Success criteria",
        "-",
        "",
        "# Risks",
        "-",
        "",
        "# Plan (milestones)",
        "1.",
        "",
        "# Evidence / proofs",
        "-",
        "",
        "# Open questions",
        "-",
        "",
        "# Next best action",
        "-",
        "",
    ]
    .join("\n")
}

fn render_decision(goal: Option<&str>) -> String {
    let context = goal.unwrap_or("").trim();
    let context_line = if context.is_empty() {
        "<fill context>".to_string()
    } else {
        context.to_string()
    };

    [
        "# Context",
        &context_line,
        "",
        "# Options",
        "- A:",
        "- B:",
        "",
        "# Decision",
        "-",
        "",
        "# Consequences",
        "-",
        "",
    ]
    .join("\n")
}

pub(crate) fn render_note_template(id: &str, goal: Option<&str>) -> Option<String> {
    match id {
        "initiative" => Some(render_initiative(goal)),
        "decision" => Some(render_decision(goal)),
        _ => None,
    }
}
