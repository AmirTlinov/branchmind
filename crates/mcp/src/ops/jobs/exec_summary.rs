#![forbid(unsafe_code)]

use crate::ops::{Envelope, OpError, OpResponse};

pub(crate) fn handle_jobs_exec_summary(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "arguments must be an object".to_string(),
                recovery: Some("Provide args as a JSON object.".to_string()),
            },
        );
    };

    const ALLOWED: &[&str] = &[
        // envelope passthrough (allowed across jobs.* strict-args mode)
        "workspace",
        "context_budget",
        "max_chars",
        "view",
        "limit",
        "task",
        "anchor",
        "stall_after_s",
        "max_regressions",
        "include_details",
    ];
    let unknown = args_obj
        .keys()
        .filter(|k| !ALLOWED.contains(&k.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        let list = unknown.join(", ");
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: format!("unknown args: {list}"),
                recovery: Some(
                    "Remove unknown args (allowed: view, limit, task, anchor, stall_after_s, max_regressions, include_details)."
                        .to_string(),
                ),
            },
        );
    }

    crate::ops::build_jobs_exec_summary(
        server,
        env.cmd.clone(),
        env.workspace.as_deref(),
        env.args.clone(),
    )
}
