#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

macro_rules! define_tasks_dispatch {
    ($($tool_name:literal => $method:ident),* $(,)?) => {
        pub(crate) fn dispatch_tasks_tool(
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
        pub(crate) fn dispatch_tasks_tool_names() -> &'static [&'static str] {
            &[$($tool_name),*]
        }
    };
}

define_tasks_dispatch! {
    "create" => tool_tasks_create,
    "bootstrap" => tool_tasks_bootstrap,
    "macro_start" => tool_tasks_macro_start,
    "macro_delegate" => tool_tasks_macro_delegate,
    "macro_fanout_jobs" => tool_tasks_macro_fanout_jobs,
    "macro_merge_report" => tool_tasks_macro_merge_report,
    "macro_close_step" => tool_tasks_macro_close_step,
    "macro_finish" => tool_tasks_macro_finish,
    "macro_create_done" => tool_tasks_macro_create_done,
    "decompose" => tool_tasks_decompose,
    "define" => tool_tasks_define,
    "note" => tool_tasks_note,
    "verify" => tool_tasks_verify,
    "done" => tool_tasks_done,
    "close_step" => tool_tasks_close_step,
    "block" => tool_tasks_block,
    "progress" => tool_tasks_progress,
    "edit" => tool_tasks_edit,
    "patch" => tool_tasks_patch,
    "delete" => tool_tasks_delete,
    "task_add" => tool_tasks_task_add,
    "task_define" => tool_tasks_task_define,
    "task_delete" => tool_tasks_task_delete,
    "evidence_capture" => tool_tasks_evidence_capture,
    "step_lease_get" => tool_tasks_step_lease_get,
    "step_lease_claim" => tool_tasks_step_lease_claim,
    "step_lease_renew" => tool_tasks_step_lease_renew,
    "step_lease_release" => tool_tasks_step_lease_release,
    "history" => tool_tasks_history,
    "undo" => tool_tasks_undo,
    "redo" => tool_tasks_redo,
    "batch" => tool_tasks_batch,
    "context" => tool_tasks_context,
    "delta" => tool_tasks_delta,
    "plan" => tool_tasks_plan,
    "planfs_init" => tool_tasks_planfs_init,
    "planfs_export" => tool_tasks_planfs_export,
    "planfs_import" => tool_tasks_planfs_import,
    "contract" => tool_tasks_contract,
    "complete" => tool_tasks_complete,
    "focus_get" => tool_tasks_focus_get,
    "focus_set" => tool_tasks_focus_set,
    "focus_clear" => tool_tasks_focus_clear,
    "radar" => tool_tasks_radar,
    "resume" => tool_tasks_resume,
    "resume_pack" => tool_tasks_resume_pack,
    "resume_super" => tool_tasks_resume_super,
    "snapshot" => tool_tasks_snapshot,
    "search" => tool_tasks_search,
    "context_pack" => tool_tasks_context_pack,
    "mindpack" => tool_tasks_mindpack,
    "mirror" => tool_tasks_mirror,
    "handoff" => tool_tasks_handoff,
    "lint" => tool_tasks_lint,
    "slices_propose_next" => tool_tasks_slices_propose_next,
    "slices_apply" => tool_tasks_slices_apply,
    "slice_open" => tool_tasks_slice_open,
    "slice_validate" => tool_tasks_slice_validate,
    "templates_list" => tool_tasks_templates_list,
    "scaffold" => tool_tasks_scaffold,
    "storage" => tool_tasks_storage,
    "jobs_create" => tool_tasks_jobs_create,
    "jobs_list" => tool_tasks_jobs_list,
    "jobs_artifact_put" => tool_tasks_jobs_artifact_put,
    "jobs_artifact_get" => tool_tasks_jobs_artifact_get,
    "jobs_radar" => tool_tasks_jobs_radar,
    "jobs_open" => tool_tasks_jobs_open,
    "jobs_proof_attach" => tool_tasks_jobs_proof_attach,
    "jobs_tail" => tool_tasks_jobs_tail,
    "jobs_claim" => tool_tasks_jobs_claim,
    "jobs_message" => tool_tasks_jobs_message,
    "jobs_report" => tool_tasks_jobs_report,
    "jobs_complete" => tool_tasks_jobs_complete,
    "jobs_requeue" => tool_tasks_jobs_requeue,
    "jobs_control_center" => tool_tasks_jobs_control_center,
    "jobs_macro_rotate_stalled" => tool_tasks_jobs_macro_rotate_stalled,
    "jobs_macro_respond_inbox" => tool_tasks_jobs_macro_respond_inbox,
    "jobs_macro_dispatch_slice" => tool_tasks_jobs_macro_dispatch_slice,
    "jobs_macro_dispatch_scout" => tool_tasks_jobs_macro_dispatch_scout,
    "jobs_macro_dispatch_builder" => tool_tasks_jobs_macro_dispatch_builder,
    "jobs_macro_dispatch_validator" => tool_tasks_jobs_macro_dispatch_validator,
    "jobs_macro_enforce_proof" => tool_tasks_jobs_macro_enforce_proof,
    "jobs_macro_sync_team" => tool_tasks_jobs_macro_sync_team,
    "jobs_pipeline_ab_slice" => tool_tasks_jobs_pipeline_ab_slice,
    "jobs_pipeline_gate" => tool_tasks_jobs_pipeline_gate,
    "jobs_pipeline_apply" => tool_tasks_jobs_pipeline_apply,
    "jobs_macro_dispatch_writer" => tool_tasks_jobs_macro_dispatch_writer,
    "jobs_pipeline_pre_validate" => tool_tasks_jobs_pipeline_pre_validate,
    "jobs_pipeline_context_review" => tool_tasks_jobs_pipeline_context_review,
    "jobs_pipeline_cascade_init" => tool_tasks_jobs_pipeline_cascade_init,
    "jobs_pipeline_cascade_advance" => tool_tasks_jobs_pipeline_cascade_advance,
    "jobs_mesh_publish" => tool_tasks_jobs_mesh_publish,
    "jobs_mesh_pull" => tool_tasks_jobs_mesh_pull,
    "jobs_mesh_ack" => tool_tasks_jobs_mesh_ack,
    "jobs_mesh_link" => tool_tasks_jobs_mesh_link,
    "runner_heartbeat" => tool_tasks_runner_heartbeat,
}
