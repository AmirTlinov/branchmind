#![forbid(unsafe_code)]

mod entry;
mod server;
mod support;
mod tools;

pub(crate) use support::*;

pub(crate) use bm_core::ids::WorkspaceId;
pub(crate) use bm_core::model::{ReasoningRef, TaskKind};
pub(crate) use bm_core::paths::StepPath;
use bm_storage::SqliteStore;
pub(crate) use bm_storage::StoreError;

const MCP_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "branchmind-rust-mcp";
const SERVER_VERSION: &str = "0.1.0";

const DEFAULT_NOTES_DOC: &str = "notes";
const DEFAULT_GRAPH_DOC: &str = "graph";
const DEFAULT_TRACE_DOC: &str = "trace";
const PIN_TAG: &str = "pinned";

pub(crate) struct McpServer {
    initialized: bool,
    store: SqliteStore,
    toolset: Toolset,
    default_workspace: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage_dir = parse_storage_dir();
    let toolset = parse_toolset();
    let default_workspace = parse_default_workspace();
    let store = SqliteStore::open(storage_dir)?;
    let mut server = McpServer::new(store, toolset, default_workspace);
    entry::run_stdio(&mut server)
}
