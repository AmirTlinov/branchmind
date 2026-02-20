#![forbid(unsafe_code)]

mod definitions;
mod dispatch;
mod markdown;
mod tool_branch;
mod tool_merge;
mod tool_think;

pub(crate) use definitions::tool_definitions;
pub(crate) use dispatch::dispatch_tool;

/// v3 advertised MCP surface: markdown-only + exactly 3 tools.
pub(crate) fn is_v1_tool(name: &str) -> bool {
    matches!(name, "think" | "branch" | "merge")
}
