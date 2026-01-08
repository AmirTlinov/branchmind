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
    workspace_lock: bool,
    project_guard: Option<String>,
    default_agent_id: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage_dir = parse_storage_dir();
    let toolset = parse_toolset();
    let default_workspace = parse_default_workspace();
    let workspace_lock = parse_workspace_lock();
    let project_guard = parse_project_guard();
    let default_agent_id_config = parse_default_agent_id_config();
    let socket_path = parse_socket_path(&storage_dir);

    if parse_daemon_mode() {
        #[cfg(unix)]
        {
            let config = entry::DaemonConfig {
                storage_dir,
                toolset,
                default_workspace,
                workspace_lock,
                project_guard,
                default_agent_id_config,
                socket_path,
            };
            return entry::run_socket_daemon(config);
        }

        #[cfg(not(unix))]
        {
            return Err("daemon mode is only supported on unix targets".into());
        }
    }

    if parse_shared_mode() {
        #[cfg(unix)]
        {
            let config = entry::SharedProxyConfig {
                storage_dir,
                toolset,
                default_workspace,
                workspace_lock,
                project_guard,
                default_agent_id_config,
                socket_path,
            };
            return entry::run_shared_proxy(config);
        }

        #[cfg(not(unix))]
        {
            return Err("shared mode is only supported on unix targets".into());
        }
    }

    let mut store = SqliteStore::open(storage_dir)?;

    let default_agent_id = match default_agent_id_config {
        Some(DefaultAgentIdConfig::Explicit(id)) => Some(id),
        Some(DefaultAgentIdConfig::Auto) => Some(store.default_agent_id_auto_get_or_create()?),
        None => None,
    };
    let mut server = McpServer::new(
        store,
        toolset,
        default_workspace,
        workspace_lock,
        project_guard,
        default_agent_id,
    );
    entry::run_stdio(&mut server)
}
