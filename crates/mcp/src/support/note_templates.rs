#![forbid(unsafe_code)]

/// Render a small, deterministic note template used by macros.
///
/// This is intentionally **std-only** and must stay free of any external I/O:
/// - no filesystem reads
/// - no network
/// - no external processes
///
/// The goal is to provide a predictable “first note” shape for common workflows.
pub(crate) fn render_note_template(template_id: &str, goal: Option<&str>) -> Option<String> {
    let id = template_id.trim().to_ascii_lowercase();
    let goal = goal.map(|s| s.trim()).filter(|s| !s.is_empty());

    match id.as_str() {
        "initiative" => {
            let mut out = String::new();
            out.push_str("# Initiative\n\n");
            if let Some(goal) = goal {
                out.push_str("## Goal\n");
                out.push_str(goal);
                out.push_str("\n\n");
            } else {
                out.push_str("## Goal\n<fill>\n\n");
            }
            out.push_str("## Context\n<fill>\n\n");
            out.push_str("## Definition of Done\n<fill>\n\n");
            out.push_str("## Risks / Unknowns\n<fill>\n");
            Some(out)
        }
        "decision" => {
            let mut out = String::new();
            out.push_str("# Decision\n\n");
            if let Some(goal) = goal {
                out.push_str("## Context\n");
                out.push_str(goal);
                out.push_str("\n\n");
            } else {
                out.push_str("## Context\n<fill>\n\n");
            }
            out.push_str("## Decision\n<fill>\n\n");
            out.push_str("## Rationale\n<fill>\n\n");
            out.push_str("## Proof / Evidence\n- CMD: <...>\n- LINK: <...>\n");
            Some(out)
        }
        _ => None,
    }
}
