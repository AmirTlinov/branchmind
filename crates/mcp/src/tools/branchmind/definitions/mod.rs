#![forbid(unsafe_code)]

use serde_json::Value;

mod anchors;
mod branches;
mod core;
mod docs;
mod graph;
mod knowledge;
mod notes_vcs;
mod packs;
mod think;
mod trace;
mod transcripts;

pub(crate) fn branchmind_tool_definitions() -> Vec<Value> {
    let mut out = Vec::new();
    out.extend(core::core_definitions());
    out.extend(anchors::anchors_definitions());
    out.extend(branches::branches_definitions());
    out.extend(notes_vcs::notes_vcs_definitions());
    out.extend(docs::docs_definitions());
    out.extend(graph::graph_definitions());
    out.extend(knowledge::knowledge_definitions());
    out.extend(think::think_definitions());
    out.extend(trace::trace_definitions());
    out.extend(packs::packs_definitions());
    out.extend(transcripts::transcripts_definitions());
    out
}
