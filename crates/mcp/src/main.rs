#![forbid(unsafe_code)]

mod entry;
mod server;
mod support;
mod tools;
mod viewer;

pub(crate) use support::*;

pub(crate) use bm_core::ids::WorkspaceId;
pub(crate) use bm_core::model::{ReasoningRef, TaskKind};
pub(crate) use bm_core::paths::StepPath;
use bm_storage::SqliteStore;
pub(crate) use bm_storage::StoreError;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

// Protocol negotiation:
// Some MCP clients are strict about the server echoing a compatible protocol version.
// We keep this at the widely deployed baseline and remain forward-compatible in behavior.
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
    workspace_explicit: bool,
    workspace_override: Option<String>,
    workspace_allowlist: Option<Vec<String>>,
    workspace_lock: bool,
    project_guard: Option<String>,
    project_guard_rebind_enabled: bool,
    default_agent_id: Option<String>,
    runner_autostart_enabled: Arc<AtomicBool>,
    runner_autostart_dry_run: bool,
    runner_autostart: Arc<Mutex<RunnerAutostartState>>,
}

#[derive(Default)]
pub(crate) struct RunnerAutostartState {
    entries: std::collections::HashMap<String, RunnerAutostartEntry>,
}

pub(crate) struct RunnerAutostartEntry {
    last_attempt_ms: i64,
    last_attempt_ok: bool,
    child: Option<std::process::Child>,
}

pub(crate) struct McpServerConfig {
    toolset: Toolset,
    default_workspace: Option<String>,
    workspace_explicit: bool,
    workspace_allowlist: Option<Vec<String>>,
    workspace_lock: bool,
    project_guard: Option<String>,
    project_guard_rebind_enabled: bool,
    default_agent_id: Option<String>,
    runner_autostart_enabled: Arc<AtomicBool>,
    runner_autostart_dry_run: bool,
    runner_autostart: Arc<Mutex<RunnerAutostartState>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ensure the compatibility fingerprint is computed at process start.
    // This prevents a long-lived daemon from accidentally observing a replaced on-disk binary
    // and reporting a misleading "new" fingerprint while still running old code.
    let _ = build_fingerprint();

    let storage_dir = parse_storage_dir();
    let toolset = parse_toolset();
    let workspace_explicit = parse_workspace_explicit();
    let workspace_allowlist = parse_workspace_allowlist();
    let default_workspace = parse_default_workspace(
        workspace_explicit.as_deref(),
        workspace_allowlist.as_deref(),
    );
    let workspace_lock =
        parse_workspace_lock(workspace_explicit.is_some(), workspace_allowlist.is_some());
    let project_guard = parse_project_guard(&storage_dir);
    let project_guard_rebind_enabled = parse_project_guard_rebind_enabled(&storage_dir);
    let default_agent_id_config = parse_default_agent_id_config();
    let socket_path = parse_socket_path(&storage_dir);
    let shared_mode = parse_shared_mode();
    let daemon_mode = parse_daemon_mode();
    let viewer_enabled = parse_viewer_enabled();
    let viewer_enabled_daemon = parse_viewer_enabled_daemon();
    let viewer_port = parse_viewer_port();
    let hot_reload_enabled = parse_hot_reload_enabled();
    let hot_reload_poll_ms = parse_hot_reload_poll_ms();
    let runner_autostart_enabled =
        parse_runner_autostart_override().unwrap_or(toolset != Toolset::Full);
    let runner_autostart_dry_run = parse_runner_autostart_dry_run();

    let workspace_recommended = viewer::recommended_workspace_from_storage_dir(&storage_dir);
    let presence_mode = if daemon_mode {
        "daemon"
    } else if shared_mode {
        "shared"
    } else {
        "stdio"
    };
    viewer::record_catalog_entry(viewer::PresenceConfig {
        storage_dir: storage_dir.clone(),
        project_guard: project_guard.clone(),
        workspace_default: default_workspace.clone(),
        workspace_recommended: Some(workspace_recommended.clone()),
        mode: presence_mode,
    });
    if !daemon_mode {
        viewer::start_presence_writer(viewer::PresenceConfig {
            storage_dir: storage_dir.clone(),
            project_guard: project_guard.clone(),
            workspace_default: default_workspace.clone(),
            workspace_recommended: Some(workspace_recommended.clone()),
            mode: if shared_mode { "shared" } else { "stdio" },
        });
    }

    if shared_mode {
        #[cfg(unix)]
        {
            let runner_autostart_enabled_shared =
                Arc::new(AtomicBool::new(runner_autostart_enabled));
            let runner_autostart_state_shared =
                Arc::new(Mutex::new(RunnerAutostartState::default()));
            let config = entry::SharedProxyConfig {
                storage_dir,
                toolset,
                default_workspace,
                workspace_explicit: workspace_explicit.is_some(),
                workspace_allowlist: workspace_allowlist.clone(),
                workspace_lock,
                project_guard,
                project_guard_rebind_enabled,
                default_agent_id_config,
                socket_path,
                viewer_enabled,
                viewer_port,
                hot_reload_enabled,
                hot_reload_poll_ms,
                runner_autostart_dry_run,
                runner_autostart_enabled_shared,
                runner_autostart_state_shared,
            };
            return entry::run_shared_proxy(config);
        }

        #[cfg(not(unix))]
        {
            return Err("shared mode is only supported on unix targets".into());
        }
    }

    if daemon_mode {
        #[cfg(unix)]
        {
            let runner_autostart_enabled_shared =
                Arc::new(AtomicBool::new(runner_autostart_enabled));
            let runner_autostart_state_shared =
                Arc::new(Mutex::new(RunnerAutostartState::default()));
            let config = entry::DaemonConfig {
                storage_dir,
                toolset,
                default_workspace,
                workspace_explicit: workspace_explicit.is_some(),
                workspace_allowlist: workspace_allowlist.clone(),
                workspace_lock,
                project_guard,
                project_guard_rebind_enabled,
                default_agent_id_config,
                socket_path,
                viewer_enabled: viewer_enabled_daemon,
                viewer_port,
                hot_reload_enabled,
                hot_reload_poll_ms,
                runner_autostart_dry_run,
                runner_autostart_enabled_shared,
                runner_autostart_state_shared,
            };
            return entry::run_socket_daemon(config);
        }

        #[cfg(not(unix))]
        {
            return Err("daemon mode is only supported on unix targets".into());
        }
    }

    let mut store = SqliteStore::open(&storage_dir)?;

    let default_agent_id = match default_agent_id_config {
        Some(DefaultAgentIdConfig::Explicit(id)) => Some(id),
        Some(DefaultAgentIdConfig::Auto) => Some(store.default_agent_id_auto_get_or_create()?),
        None => None,
    };
    let runner_autostart_enabled = Arc::new(AtomicBool::new(runner_autostart_enabled));
    let runner_autostart_state = Arc::new(Mutex::new(RunnerAutostartState::default()));

    let mut server = McpServer::new(
        store,
        McpServerConfig {
            toolset,
            default_workspace,
            workspace_explicit: workspace_explicit.is_some(),
            workspace_allowlist: workspace_allowlist.clone(),
            workspace_lock,
            project_guard,
            project_guard_rebind_enabled,
            default_agent_id,
            runner_autostart_enabled: runner_autostart_enabled.clone(),
            runner_autostart_dry_run,
            runner_autostart: runner_autostart_state.clone(),
        },
    );
    if viewer_enabled {
        let viewer_config = viewer::ViewerConfig {
            storage_dir: storage_dir.clone(),
            workspace: server.default_workspace.clone(),
            project_guard: server.project_guard.clone(),
            port: viewer_port,
            runner_autostart_enabled: Some(runner_autostart_enabled.clone()),
            runner_autostart_dry_run: server.runner_autostart_dry_run,
            runner_autostart: Some(runner_autostart_state.clone()),
        };
        // Viewer is optional and must not break MCP startup.
        let _ = viewer::start_viewer(viewer_config);
        // Operational UX: if another session currently owns :7331, retry periodically so the
        // viewer self-heals when the port becomes free (multi-Codex sessions).
        let retry_config = viewer::ViewerConfig {
            storage_dir: storage_dir.clone(),
            workspace: server.default_workspace.clone(),
            project_guard: server.project_guard.clone(),
            port: viewer_port,
            runner_autostart_enabled: Some(runner_autostart_enabled.clone()),
            runner_autostart_dry_run: server.runner_autostart_dry_run,
            runner_autostart: Some(runner_autostart_state.clone()),
        };
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let _ = viewer::start_viewer(retry_config.clone());
            }
        });
    }
    entry::run_stdio(&mut server, hot_reload_enabled, hot_reload_poll_ms)
}
