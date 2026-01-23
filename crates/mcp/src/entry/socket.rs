#![forbid(unsafe_code)]

use crate::entry::framing::{parse_request, read_content_length_frame, write_content_length_json};
use crate::{DefaultAgentIdConfig, McpServer, Toolset};
use bm_storage::SqliteStore;
use serde_json::{Value, json};
use std::io::{BufReader, BufWriter};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct DaemonConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) toolset: Toolset,
    pub(crate) default_workspace: Option<String>,
    pub(crate) workspace_lock: bool,
    pub(crate) project_guard: Option<String>,
    pub(crate) project_guard_rebind_enabled: bool,
    pub(crate) default_agent_id_config: Option<DefaultAgentIdConfig>,
    pub(crate) socket_path: PathBuf,
    pub(crate) viewer_enabled: bool,
    pub(crate) viewer_port: u16,
    pub(crate) hot_reload_enabled: bool,
    pub(crate) hot_reload_poll_ms: u64,
    pub(crate) runner_autostart_dry_run: bool,
    pub(crate) runner_autostart_enabled_shared: Arc<AtomicBool>,
    pub(crate) runner_autostart_state_shared: Arc<Mutex<crate::RunnerAutostartState>>,
}

pub(crate) fn run_socket_daemon(config: DaemonConfig) -> Result<(), Box<dyn std::error::Error>> {
    if UnixStream::connect(&config.socket_path).is_ok() {
        return Ok(());
    }

    if config.socket_path.exists() {
        let _ = std::fs::remove_file(&config.socket_path);
    }

    if config.viewer_enabled {
        let viewer_config = crate::viewer::ViewerConfig {
            storage_dir: config.storage_dir.clone(),
            workspace: config.default_workspace.clone(),
            project_guard: config.project_guard.clone(),
            port: config.viewer_port,
            runner_autostart_enabled: Some(config.runner_autostart_enabled_shared.clone()),
            runner_autostart_dry_run: config.runner_autostart_dry_run,
            runner_autostart: Some(config.runner_autostart_state_shared.clone()),
        };
        // Viewer is optional and must not break daemon startup.
        let _ = crate::viewer::start_viewer(viewer_config);
    }

    let listener = match UnixListener::bind(&config.socket_path) {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&config.socket_path).is_ok() {
                return Ok(());
            }
            return Err(err.into());
        }
        Err(err) => return Err(err.into()),
    };

    let hot_reload = crate::HotReload::start(
        config.hot_reload_enabled,
        Duration::from_millis(config.hot_reload_poll_ms),
    );
    let _ = listener.set_nonblocking(true);

    let config = Arc::new(config);

    loop {
        let _ = hot_reload.maybe_exec_now();

        match listener.accept() {
            Ok((stream, _addr)) => {
                let config = Arc::clone(&config);
                thread::spawn(move || {
                    let _ = handle_connection(stream, config);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => continue,
        }
    }
}

fn handle_connection(
    stream: UnixStream,
    config: Arc<DaemonConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    let mut server = build_server(&config)?;

    loop {
        let Some(body) = read_content_length_frame(&mut reader, None)? else {
            break;
        };

        let response: Option<Value> = match parse_request(&body) {
            Ok(request) => {
                // Internal: allow the shared proxy to terminate a stale/unhealthy daemon so a fresh
                // build can take over the stable socket path.
                //
                // This is intentionally not part of the tool surface (not discoverable via tools/list).
                if request.method == "branchmind/daemon_shutdown" {
                    let resp = crate::json_rpc_response(request.id, json!({ "ok": true }));
                    let _ = std::fs::remove_file(&config.socket_path);
                    write_content_length_json(&mut writer, &resp)?;
                    std::process::exit(0);
                }

                // Internal: allow the shared proxy to verify that a reused daemon matches the
                // current session's config (storage_dir / project_guard / defaults). This prevents
                // cross-project drift when multiple repos share a stable socket path.
                if request.method == "branchmind/daemon_info" {
                    let storage_dir = std::fs::canonicalize(&config.storage_dir)
                        .unwrap_or_else(|_| config.storage_dir.clone())
                        .to_string_lossy()
                        .to_string();
                    Some(crate::json_rpc_response(
                        request.id,
                        json!({
                            "fingerprint": crate::build_fingerprint(),
                            "storage_dir": storage_dir,
                            "toolset": config.toolset.as_str(),
                            "default_workspace": config.default_workspace,
                            "workspace_lock": config.workspace_lock,
                            "project_guard": config.project_guard,
                            "viewer_enabled": config.viewer_enabled,
                            "viewer_port": config.viewer_port
                        }),
                    ))
                } else {
                    server.handle(request)
                }
            }
            Err(err) => Some(err),
        };

        if let Some(resp) = response {
            write_content_length_json(&mut writer, &resp)?;
        }
    }

    Ok(())
}

fn build_server(config: &DaemonConfig) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut store = SqliteStore::open(&config.storage_dir)?;
    let default_agent_id = match &config.default_agent_id_config {
        Some(DefaultAgentIdConfig::Explicit(id)) => Some(id.clone()),
        Some(DefaultAgentIdConfig::Auto) => Some(store.default_agent_id_auto_get_or_create()?),
        None => None,
    };

    Ok(McpServer::new(
        store,
        crate::McpServerConfig {
            toolset: config.toolset,
            default_workspace: config.default_workspace.clone(),
            workspace_lock: config.workspace_lock,
            project_guard: config.project_guard.clone(),
            project_guard_rebind_enabled: config.project_guard_rebind_enabled,
            default_agent_id,
            runner_autostart_enabled: config.runner_autostart_enabled_shared.clone(),
            runner_autostart_dry_run: config.runner_autostart_dry_run,
            runner_autostart: config.runner_autostart_state_shared.clone(),
        },
    ))
}
