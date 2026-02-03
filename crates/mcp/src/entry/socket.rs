#![forbid(unsafe_code)]

use crate::entry::framing::{parse_request, read_content_length_frame, write_content_length_json};
use crate::{DefaultAgentIdConfig, McpServer, ResponseVerbosity, Toolset};
use bm_storage::SqliteStore;
use serde_json::{Value, json};
use std::io::{BufReader, BufWriter};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct DaemonConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) toolset: Toolset,
    pub(crate) response_verbosity: ResponseVerbosity,
    pub(crate) dx_mode: bool,
    pub(crate) ux_proof_v2_enabled: bool,
    pub(crate) knowledge_autolint_enabled: bool,
    pub(crate) note_promote_enabled: bool,
    pub(crate) default_workspace: Option<String>,
    pub(crate) workspace_explicit: bool,
    pub(crate) workspace_allowlist: Option<Vec<String>>,
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
    if let Ok(stream) = UnixStream::connect(&config.socket_path) {
        match daemon_is_compatible(&stream, &config) {
            Ok(true) => return Ok(()),
            Ok(false) => {
                let _ = try_shutdown_daemon(&stream);
                let _ = std::fs::remove_file(&config.socket_path);
            }
            // Flagship stability: if we can't probe the daemon (timeout/parse), treat it as
            // already running and do not attempt a takeover. Killing a healthy daemon breaks
            // other sessions.
            Err(_) => return Ok(()),
        }
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

    // Flagship stability: avoid leaving orphaned long-lived daemons around when no sessions use
    // them anymore. Shared proxies set `BRANCHMIND_MCP_DAEMON_IDLE_EXIT_SECS` on spawned daemons.
    // Manual `--daemon` runs keep the daemon persistent unless configured explicitly.
    let idle_exit_after = std::env::var("BRANCHMIND_MCP_DAEMON_IDLE_EXIT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs);

    let config = Arc::new(config);
    let socket_path = config.socket_path.clone();
    let active_connections = Arc::new(AtomicUsize::new(0));
    let mut idle_since: Option<std::time::Instant> = None;

    loop {
        let _ = hot_reload.maybe_exec_now();
        // Flagship stability: if the socket path is unlinked while this daemon is running
        // (e.g., a shared proxy forces recovery), terminate. Keeping a daemon alive without
        // an address leads to process explosions and cross-session confusion.
        if !socket_path.exists() {
            return Ok(());
        }

        if let Some(idle_after) = idle_exit_after {
            let active = active_connections.load(Ordering::SeqCst);
            if active == 0 {
                idle_since.get_or_insert_with(std::time::Instant::now);
                if idle_since
                    .as_ref()
                    .is_some_and(|since| since.elapsed() >= idle_after)
                {
                    let _ = std::fs::remove_file(&socket_path);
                    return Ok(());
                }
            } else {
                idle_since = None;
            }
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                // The listener is set to nonblocking so the daemon main loop can poll hot reload,
                // idle exit conditions, etc. Ensure per-connection streams are blocking so we
                // don't accidentally treat "no data yet" as EOF/error and close the transport.
                let _ = stream.set_nonblocking(false);
                let config = Arc::clone(&config);
                let counter = Arc::clone(&active_connections);
                counter.fetch_add(1, Ordering::SeqCst);
                thread::spawn(move || {
                    struct ConnGuard {
                        counter: Arc<AtomicUsize>,
                    }
                    impl Drop for ConnGuard {
                        fn drop(&mut self) {
                            self.counter.fetch_sub(1, Ordering::SeqCst);
                        }
                    }

                    let _guard = ConnGuard { counter };
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
                                        "compat_fingerprint": crate::build_compat_fingerprint(),
                                        "fingerprint": crate::build_fingerprint(),
                                        "build_time_ms": crate::binary_build_time_ms().unwrap_or(0),
                                        "storage_dir": storage_dir,
                        "toolset": config.toolset.as_str(),
                        "response_verbosity": config.response_verbosity.as_str(),
                        "dx_mode": config.dx_mode,
                        "default_workspace": config.default_workspace,
                        "workspace_explicit": config.workspace_explicit,
                        "workspace_allowlist": config.workspace_allowlist,
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
            response_verbosity: config.response_verbosity,
            dx_mode: config.dx_mode,
            ux_proof_v2_enabled: config.ux_proof_v2_enabled,
            knowledge_autolint_enabled: config.knowledge_autolint_enabled,
            note_promote_enabled: config.note_promote_enabled,
            default_workspace: config.default_workspace.clone(),
            workspace_explicit: config.workspace_explicit,
            workspace_allowlist: config.workspace_allowlist.clone(),
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

fn daemon_is_compatible(
    stream: &UnixStream,
    config: &DaemonConfig,
) -> Result<bool, Box<dyn std::error::Error>> {
    let info = probe_daemon_info(stream)?;
    let local_compat = crate::build_compat_fingerprint();
    let daemon_compat = info
        .get("compat_fingerprint")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            info.get("fingerprint")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|full| full.split(".bin.").next().unwrap_or(full).to_string())
        });
    let Some(daemon_compat) = daemon_compat else {
        return Ok(false);
    };
    if daemon_compat != local_compat {
        return Ok(false);
    }

    let daemon_build_time_ms = info
        .get("build_time_ms")
        .and_then(|v| v.as_u64())
        .filter(|ms| *ms > 0);
    let local_build_time_ms = crate::binary_build_time_ms();
    if let (Some(daemon_ms), Some(local_ms)) = (daemon_build_time_ms, local_build_time_ms)
        && daemon_ms < local_ms
    {
        // Same compat version but older binary; force replacement.
        return Ok(false);
    }

    let daemon_storage_dir = info
        .get("storage_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let local_storage_dir = std::fs::canonicalize(&config.storage_dir)
        .unwrap_or_else(|_| config.storage_dir.clone())
        .to_string_lossy()
        .to_string();
    if daemon_storage_dir != local_storage_dir {
        return Ok(false);
    }

    let daemon_toolset = info
        .get("toolset")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if daemon_toolset != config.toolset.as_str() {
        return Ok(false);
    }

    let daemon_verbosity = info
        .get("response_verbosity")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !daemon_verbosity.is_empty() && daemon_verbosity != config.response_verbosity.as_str() {
        return Ok(false);
    }
    let daemon_dx_mode = info
        .get("dx_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if daemon_dx_mode != config.dx_mode {
        return Ok(false);
    }

    let daemon_workspace_explicit = info
        .get("workspace_explicit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if daemon_workspace_explicit != config.workspace_explicit {
        return Ok(false);
    }

    let daemon_workspace_lock = info
        .get("workspace_lock")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if daemon_workspace_lock != config.workspace_lock {
        return Ok(false);
    }

    let daemon_default_workspace = info
        .get("default_workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if daemon_default_workspace != config.default_workspace {
        return Ok(false);
    }

    let daemon_workspace_allowlist = info
        .get("workspace_allowlist")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
    if !allowlist_equivalent(&daemon_workspace_allowlist, &config.workspace_allowlist) {
        return Ok(false);
    }

    let daemon_project_guard = info
        .get("project_guard")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if daemon_project_guard != config.project_guard {
        return Ok(false);
    }

    let daemon_viewer_enabled = info
        .get("viewer_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if daemon_viewer_enabled != config.viewer_enabled {
        return Ok(false);
    }
    if daemon_viewer_enabled {
        let daemon_viewer_port = info.get("viewer_port").and_then(|v| v.as_u64());
        if daemon_viewer_port != Some(config.viewer_port as u64) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn allowlist_equivalent(a: &Option<Vec<String>>, b: &Option<Vec<String>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            let mut left = left.clone();
            let mut right = right.clone();
            left.sort();
            left.dedup();
            right.sort();
            right.dedup();
            left == right
        }
        _ => false,
    }
}

fn probe_daemon_info(
    stream: &UnixStream,
) -> Result<serde_json::Map<String, Value>, Box<dyn std::error::Error>> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "branchmind/daemon_info",
        "params": {}
    });
    let resp = send_internal_request(stream, &req, Duration::from_millis(400))?;
    if resp.get("error").is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "daemon_info unavailable",
        )
        .into());
    }
    resp.get("result")
        .and_then(|v| v.as_object())
        .cloned()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "daemon_info missing result",
            )
            .into()
        })
}

fn send_internal_request(
    stream: &UnixStream,
    request: &Value,
    timeout: Duration,
) -> Result<Value, Box<dyn std::error::Error>> {
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream.try_clone()?);
    write_content_length_json(&mut writer, request)?;
    let Some(resp_body) = read_content_length_frame(&mut reader, None)? else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "daemon closed during internal request",
        )
        .into());
    };
    let value = serde_json::from_slice::<Value>(&resp_body)?;

    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);

    Ok(value)
}

fn try_shutdown_daemon(stream: &UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "branchmind/daemon_shutdown",
        "params": {}
    });
    let _ = send_internal_request(stream, &req, Duration::from_millis(400));
    Ok(())
}
