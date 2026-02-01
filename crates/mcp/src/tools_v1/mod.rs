#![forbid(unsafe_code)]

mod definitions;
mod dispatch;
mod docs_ops;
mod graph_ops;
mod jobs_ops;
mod open;
mod status;
mod system_ops;
mod tasks_ops;
mod think_ops;
mod vcs_ops;
mod workspace_ops;

pub(crate) use definitions::tool_definitions;
pub(crate) use dispatch::dispatch_tool;

/// v1 advertised MCP surface: exactly 10 portal tools.
///
/// Note: the server may accept namespace-prefixed tool names for MCP interoperability
/// (e.g. `branchmind.status` / `branchmind/status`), but legacy tool names are rejected
/// to avoid UX entropy.
pub(crate) fn is_v1_tool(name: &str) -> bool {
    matches!(
        name,
        "status"
            | "open"
            | "workspace"
            | "tasks"
            | "jobs"
            | "think"
            | "graph"
            | "vcs"
            | "docs"
            | "system"
    )
}
