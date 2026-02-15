#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

macro_rules! define_branchmind_dispatch {
    ($($tool_name:literal => $method:ident),* $(,)?) => {
        pub(crate) fn dispatch_branchmind_tool(
            server: &mut McpServer,
            name: &str,
            args: Value,
        ) -> Option<Value> {
            let resp = match name {
                $($tool_name => server.$method(args),)*
                _ => return None,
            };
            Some(resp)
        }

        #[cfg(test)]
        pub(crate) fn dispatch_branchmind_tool_names() -> &'static [&'static str] {
            &[$($tool_name),*]
        }
    };
}

define_branchmind_dispatch! {
    "init" => tool_branchmind_init,
    "status" => tool_branchmind_status,
    "workspace_use" => tool_branchmind_workspace_use,
    "workspace_reset" => tool_branchmind_workspace_reset,
    "workspace_list" => tool_branchmind_workspace_list,
    "open" => tool_branchmind_open,
    "help" => tool_branchmind_help,
    "skill" => tool_branchmind_skill,
    "diagnostics" => tool_branchmind_diagnostics,
    "anchors_list" => tool_branchmind_anchors_list,
    "anchor_snapshot" => tool_branchmind_anchor_snapshot,
    "anchor_resolve" => tool_branchmind_anchor_resolve,
    "atlas_suggest" => tool_branchmind_atlas_suggest,
    "macro_atlas_apply" => tool_branchmind_macro_atlas_apply,
    "atlas_bindings_list" => tool_branchmind_atlas_bindings_list,
    "macro_anchor_note" => tool_branchmind_macro_anchor_note,
    "anchors_export" => tool_branchmind_anchors_export,
    "anchors_rename" => tool_branchmind_anchors_rename,
    "anchors_bootstrap" => tool_branchmind_anchors_bootstrap,
    "anchors_merge" => tool_branchmind_anchors_merge,
    "anchors_lint" => tool_branchmind_anchors_lint,
    "branch_create" => tool_branchmind_branch_create,
    "macro_branch_note" => tool_branchmind_macro_branch_note,
    "branch_list" => tool_branchmind_branch_list,
    "checkout" => tool_branchmind_checkout,
    "branch_rename" => tool_branchmind_branch_rename,
    "branch_delete" => tool_branchmind_branch_delete,
    "notes_commit" => tool_branchmind_notes_commit,
    "commit" => tool_branchmind_commit,
    "log" => tool_branchmind_log,
    "docs_list" => tool_branchmind_docs_list,
    "tag_create" => tool_branchmind_tag_create,
    "tag_list" => tool_branchmind_tag_list,
    "tag_delete" => tool_branchmind_tag_delete,
    "reflog" => tool_branchmind_reflog,
    "reset" => tool_branchmind_reset,
    "show" => tool_branchmind_show,
    "diff" => tool_branchmind_diff,
    "merge" => tool_branchmind_merge,
    "graph_apply" => tool_branchmind_graph_apply,
    "graph_query" => tool_branchmind_graph_query,
    "graph_validate" => tool_branchmind_graph_validate,
    "graph_diff" => tool_branchmind_graph_diff,
    "graph_merge" => tool_branchmind_graph_merge,
    "graph_conflicts" => tool_branchmind_graph_conflicts,
    "graph_conflict_show" => tool_branchmind_graph_conflict_show,
    "graph_conflict_resolve" => tool_branchmind_graph_conflict_resolve,
    "think_template" => tool_branchmind_think_template,
    "think_card" => tool_branchmind_think_card,
    "think_macro_counter_hypothesis_stub" => tool_branchmind_think_macro_counter_hypothesis_stub,
    "think_add_hypothesis" => tool_branchmind_think_add_hypothesis,
    "think_add_question" => tool_branchmind_think_add_question,
    "think_add_test" => tool_branchmind_think_add_test,
    "think_add_note" => tool_branchmind_think_add_note,
    "think_add_decision" => tool_branchmind_think_add_decision,
    "think_add_evidence" => tool_branchmind_think_add_evidence,
    "think_add_frame" => tool_branchmind_think_add_frame,
    "think_add_update" => tool_branchmind_think_add_update,
    "think_context" => tool_branchmind_think_context,
    "think_pipeline" => tool_branchmind_think_pipeline,
    "think_query" => tool_branchmind_think_query,
    "think_pack" => tool_branchmind_think_pack,
    "think_frontier" => tool_branchmind_think_frontier,
    "think_next" => tool_branchmind_think_next,
    "think_link" => tool_branchmind_think_link,
    "think_set_status" => tool_branchmind_think_set_status,
    "think_pin" => tool_branchmind_think_pin,
    "think_pins" => tool_branchmind_think_pins,
    "think_publish" => tool_branchmind_think_publish,
    "think_nominal_merge" => tool_branchmind_think_nominal_merge,
    "think_playbook" => tool_branchmind_think_playbook,
    "think_subgoal_open" => tool_branchmind_think_subgoal_open,
    "think_subgoal_close" => tool_branchmind_think_subgoal_close,
    "think_watch" => tool_branchmind_think_watch,
    "think_lint" => tool_branchmind_think_lint,
    "trace_step" => tool_branchmind_trace_step,
    "trace_sequential_step" => tool_branchmind_trace_sequential_step,
    "trace_hydrate" => tool_branchmind_trace_hydrate,
    "trace_validate" => tool_branchmind_trace_validate,
    "context_pack" => tool_branchmind_context_pack,
    "context_pack_export" => tool_branchmind_context_pack_export,
    "export" => tool_branchmind_export,
    "transcripts_search" => tool_branchmind_transcripts_search,
    "transcripts_open" => tool_branchmind_transcripts_open,
    "transcripts_digest" => tool_branchmind_transcripts_digest,
}
