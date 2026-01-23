#![forbid(unsafe_code)]

mod assets;
mod detail;
mod registry;
mod snapshot;

pub(crate) use registry::record_catalog_entry;
pub(crate) use registry::sync_catalog_from_presence;
pub(crate) use registry::sync_catalog_from_scan;
pub(crate) use registry::{PresenceConfig, list_projects, lookup_project, start_presence_writer};

use crate::{now_ms_i64, now_rfc3339};
use bm_storage::{SqliteStore, StoreError};
#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct ViewerConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) workspace: Option<String>,
    pub(crate) project_guard: Option<String>,
    pub(crate) port: u16,
    pub(crate) runner_autostart_enabled: Option<Arc<AtomicBool>>,
    pub(crate) runner_autostart_dry_run: bool,
    pub(crate) runner_autostart: Option<Arc<Mutex<crate::RunnerAutostartState>>>,
}

pub(crate) fn start_viewer(config: ViewerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let local_fingerprint = crate::build_fingerprint();
    let mut takeover_attempts: u8 = 0;
    let mut forced_kill_attempted = false;
    let listener = loop {
        match TcpListener::bind(("127.0.0.1", config.port)) {
            Ok(listener) => break listener,
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                // If the port is already occupied, try to detect whether it is an existing
                // BranchMind viewer. If so, do not fail: the UI is multi-project and can show
                // any project discovered via the registry/catalog.
                //
                // If the existing viewer looks stale (no response), best-effort: try to nudge a
                // stopped `bm_mcp` process back to life so the page doesn't "hang forever".
                //
                // If the existing viewer is a different build, best-effort: ask it to shut down so
                // the newest build can take over. (No store mutation; viewer-only.)
                match probe_viewer_about(config.port).ok().flatten() {
                    Some(info) => {
                        if info.fingerprint.as_deref() == Some(local_fingerprint.as_str()) {
                            return Ok(());
                        }
                        if let Some(target) = info.fingerprint.as_deref() {
                            takeover_attempts = takeover_attempts.saturating_add(1);
                            let _ = request_viewer_shutdown(config.port, target);
                            std::thread::sleep(Duration::from_millis(80));
                            if takeover_attempts < 6 {
                                continue;
                            }
                            if !forced_kill_attempted {
                                forced_kill_attempted = true;
                                terminate_existing_viewer_process(config.port);
                                std::thread::sleep(Duration::from_millis(80));
                                continue;
                            }
                            return Ok(());
                        }
                        // Legacy viewer without a fingerprint (older build). Best-effort: replace it.
                        if forced_kill_attempted {
                            return Ok(());
                        }
                        forced_kill_attempted = true;
                        terminate_existing_viewer_process(config.port);
                        std::thread::sleep(Duration::from_millis(80));
                        continue;
                    }
                    None => {
                        nudge_existing_viewer(config.port);
                        if probe_viewer_about(config.port).ok().flatten().is_some() {
                            return Ok(());
                        }
                        if forced_kill_attempted {
                            // Something else owns the port (or is unresponsive). Don't break MCP startup.
                            return Ok(());
                        }
                        forced_kill_attempted = true;
                        terminate_existing_viewer_process(config.port);
                        std::thread::sleep(Duration::from_millis(80));
                        continue;
                    }
                }
            }
            Err(err) => return Err(err.into()),
        }
    };
    sync_catalog_from_presence();
    // Best-effort background scan to seed the durable project catalog with any on-disk stores.
    // This makes the multi-project selector useful even when older sessions didn't emit presence.
    {
        let storage_dir = config.storage_dir.clone();
        std::thread::spawn(move || {
            sync_catalog_from_scan(&storage_dir);
        });
    }
    let store = SqliteStore::open(&config.storage_dir)?;

    let shutdown_thread = Arc::new(AtomicBool::new(false));
    std::thread::spawn(move || {
        let primary_dir_canon = std::fs::canonicalize(&config.storage_dir)
            .unwrap_or_else(|_| config.storage_dir.clone());
        let mut stores = ViewerStores {
            primary_dir_canon,
            primary: store,
            external: HashMap::new(),
        };
        let _ = run_viewer(listener, &mut stores, config, shutdown_thread);
    });

    Ok(())
}

#[derive(Debug)]
struct ViewerAboutInfo {
    fingerprint: Option<String>,
}

fn probe_viewer_about(port: u16) -> std::io::Result<Option<ViewerAboutInfo>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    stream.write_all(b"GET /api/about HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")?;

    let mut buf = [0u8; 4096];
    let read = stream.read(&mut buf)?;
    if read == 0 {
        return Ok(None);
    }

    let Some(header_end) = buf[..read]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
    else {
        return Ok(None);
    };
    let body = &buf[header_end..read];
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return Ok(None);
    };
    let recommended = payload
        .get("workspace_recommended")
        .and_then(|v| v.as_str());
    if recommended.is_none() {
        return Ok(None);
    }
    let fingerprint = payload
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    Ok(Some(ViewerAboutInfo { fingerprint }))
}

fn request_viewer_shutdown(port: u16, fingerprint: &str) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
    let body = json!({ "fingerprint": fingerprint }).to_string();
    let request = format!(
        "POST /api/internal/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes())?;
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
    Ok(())
}

fn nudge_existing_viewer(port: u16) {
    let Some(pid) = pid_listening_on_port(port) else {
        return;
    };
    if !process_is_bm_mcp(pid) {
        return;
    }
    if !process_is_stopped(pid) {
        return;
    }
    resume_process(pid);
}

fn terminate_existing_viewer_process(port: u16) {
    let Some(pid) = pid_listening_on_port(port) else {
        return;
    };
    if pid == std::process::id() {
        return;
    }
    if !process_is_bm_mcp(pid) {
        return;
    }

    if process_is_stopped(pid) {
        resume_process(pid);
    }
    terminate_process(pid, false);

    let deadline = Instant::now() + Duration::from_millis(350);
    while Instant::now() < deadline {
        if inode_for_tcp_listen(port).is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(40));
    }

    terminate_process(pid, true);
}

#[cfg(unix)]
fn resume_process(pid: u32) {
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGCONT);
}

#[cfg(not(unix))]
fn resume_process(_pid: u32) {}

#[cfg(unix)]
fn terminate_process(pid: u32, force: bool) {
    let signal = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };
    let _ = kill(Pid::from_raw(pid as i32), signal);
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32, _force: bool) {}

fn pid_listening_on_port(port: u16) -> Option<u32> {
    let inode = inode_for_tcp_listen(port)?;
    pid_holding_inode(inode)
}

fn inode_for_tcp_listen(port: u16) -> Option<u64> {
    let needle = format!("0100007F:{port:04X}");
    let text = std::fs::read_to_string("/proc/net/tcp").ok()?;
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 {
            continue;
        }
        let local = cols[1];
        let state = cols[3];
        if state != "0A" {
            continue;
        }
        if !local.eq_ignore_ascii_case(&needle) {
            continue;
        }
        let inode_str = cols[9];
        if let Ok(inode) = inode_str.parse::<u64>() {
            return Some(inode);
        }
    }
    None
}

fn pid_holding_inode(inode: u64) -> Option<u32> {
    let needle = format!("socket:[{inode}]");
    let proc = std::fs::read_dir("/proc").ok()?;
    for entry in proc.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.is_empty() || !name.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let fd_dir = entry.path().join("fd");
        let Ok(fds) = std::fs::read_dir(fd_dir) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            if target.to_string_lossy() == needle {
                return Some(pid);
            }
        }
    }
    None
}

fn process_is_bm_mcp(pid: u32) -> bool {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    let Some(exe) = exe else {
        return false;
    };
    let Some(name) = exe.file_name().and_then(|v| v.to_str()) else {
        return false;
    };
    if name == "bm_mcp" {
        return true;
    }
    // Linux may report a deleted executable as `bm_mcp (deleted)`, which still belongs to us and
    // should be eligible for best-effort takeover/shutdown in local dev workflows.
    if let Some(rest) = name.strip_prefix("bm_mcp") {
        return rest.trim_start().starts_with('(');
    }
    false
}

fn process_is_stopped(pid: u32) -> bool {
    let text = std::fs::read_to_string(format!("/proc/{pid}/status")).ok();
    let Some(text) = text else {
        return false;
    };
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("State:") else {
            continue;
        };
        let state = rest.trim().chars().next();
        return matches!(state, Some('T'));
    }
    false
}

struct ViewerStores {
    primary_dir_canon: PathBuf,
    primary: SqliteStore,
    external: HashMap<PathBuf, SqliteStore>,
}

impl ViewerStores {
    fn store_for(&mut self, storage_dir: &Path) -> Result<&mut SqliteStore, StoreError> {
        let canonical =
            std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
        if canonical == self.primary_dir_canon {
            return Ok(&mut self.primary);
        }
        if !self.external.contains_key(&canonical) {
            let store = SqliteStore::open_read_only(&canonical)?;
            self.external.insert(canonical.clone(), store);
        }
        Ok(self
            .external
            .get_mut(&canonical)
            .expect("store must exist after insert"))
    }
}

fn run_viewer(
    listener: TcpListener,
    stores: &mut ViewerStores,
    config: ViewerConfig,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = handle_connection(stream, stores, &config, &shutdown);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    stores: &mut ViewerStores,
    config: &ViewerConfig,
    shutdown: &Arc<AtomicBool>,
) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };

    let method = request.method.as_str();
    if method != "GET" && method != "HEAD" && method != "POST" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method not allowed.",
            false,
        );
    }

    let workspace_raw = extract_query_param_raw(&request.path, "workspace");
    let workspace_override = workspace_raw
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| normalize_workspace_param(&value));

    let project_raw = extract_query_param_raw(&request.path, "project");
    let project_override = project_raw
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| normalize_project_guard_param(&value));
    let project_param_invalid = project_raw.is_some() && project_override.is_none();
    let project_info = project_override.as_deref().and_then(lookup_project);
    let project_unknown = project_override.is_some() && project_info.is_none();

    let trace_cursor = extract_query_param_raw(&request.path, "trace_cursor")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| parse_cursor_param(&value));
    let notes_cursor = extract_query_param_raw(&request.path, "notes_cursor")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| parse_cursor_param(&value));
    let path = normalize_path(&request.path);

    let mut request_storage_dir = config.storage_dir.clone();
    let mut request_config = config.clone();
    if let Some(project) = project_info.as_ref() {
        request_storage_dir = project.storage_dir.clone();
        let is_current = config.project_guard.as_deref() == Some(project.project_guard.as_str());
        request_config = ViewerConfig {
            storage_dir: project.storage_dir.clone(),
            workspace: project
                .workspace_default
                .clone()
                .or_else(|| project.workspace_recommended.clone()),
            project_guard: Some(project.project_guard.clone()),
            port: config.port,
            runner_autostart_enabled: if is_current {
                config.runner_autostart_enabled.clone()
            } else {
                None
            },
            runner_autostart_dry_run: if is_current {
                config.runner_autostart_dry_run
            } else {
                false
            },
            runner_autostart: if is_current {
                config.runner_autostart.clone()
            } else {
                None
            },
        };
    }

    match path.as_str() {
        "/" | "/index.html" => write_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            assets::INDEX_HTML.as_bytes(),
            method == "HEAD",
        ),
        "/app.css" => write_response(
            &mut stream,
            "200 OK",
            "text/css; charset=utf-8",
            assets::APP_CSS.as_bytes(),
            method == "HEAD",
        ),
        "/app.js" => write_response(
            &mut stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            assets::APP_JS.as_bytes(),
            method == "HEAD",
        ),
        "/api/projects" => {
            let projects = list_projects()
                .into_iter()
                .map(|project| {
                    json!({
                        "project_guard": project.project_guard,
                        "label": project.label,
                        "storage_dir": project.storage_dir.to_string_lossy(),
                        "workspace_default": project.workspace_default,
                        "workspace_recommended": project.workspace_recommended,
                        "updated_at_ms": project.updated_at_ms,
                        "stale": project.stale,
                        "store_present": project.store_present,
                        "is_temp": project.is_temp
                    })
                })
                .collect::<Vec<_>>();
            let body = json!({
                "generated_at": now_rfc3339(),
                "generated_at_ms": now_ms_i64(),
                "current_project_guard": config.project_guard.as_deref(),
                "current_label": recommended_workspace_from_storage_dir(&config.storage_dir),
                "current_storage_dir": config.storage_dir.to_string_lossy(),
                "projects": projects
            })
            .to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
        "/api/about" => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let recommended = recommended_workspace_from_storage_dir(&request_config.storage_dir);
            let body = json!({
                "fingerprint": crate::build_fingerprint(),
                "project_guard": request_config.project_guard.as_deref(),
                "workspace_default": request_config.workspace.as_deref(),
                "workspace_recommended": recommended,
            })
            .to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
        "/api/workspaces" => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let store = match stores.store_for(&request_storage_dir) {
                Ok(store) => store,
                Err(err) => {
                    let recovery = err.to_string();
                    return write_api_error(
                        &mut stream,
                        "500 Internal Server Error",
                        "STORE_ERROR",
                        "Unable to open store.",
                        Some(recovery.as_str()),
                        method == "HEAD",
                    );
                }
            };

            const MAX_WORKSPACES: usize = 200;
            let workspaces = match store.list_workspaces(MAX_WORKSPACES, 0) {
                Ok(rows) => rows
                    .into_iter()
                    .map(|row| {
                        json!({
                            "workspace": row.workspace,
                            "created_at_ms": row.created_at_ms,
                            "project_guard": row.project_guard
                        })
                    })
                    .collect::<Vec<_>>(),
                Err(err) => {
                    let recovery = err.to_string();
                    return write_api_error(
                        &mut stream,
                        "500 Internal Server Error",
                        "STORE_ERROR",
                        "Unable to list workspaces.",
                        Some(recovery.as_str()),
                        method == "HEAD",
                    );
                }
            };
            let recommended = recommended_workspace_from_storage_dir(&request_config.storage_dir);
            let body = json!({
                "generated_at": now_rfc3339(),
                "generated_at_ms": now_ms_i64(),
                "project_guard": request_config.project_guard.as_deref(),
                "workspace_default": request_config.workspace.as_deref(),
                "workspace_recommended": recommended,
                "workspaces": workspaces
            })
            .to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
        "/api/internal/shutdown" if method == "POST" => {
            let expected = crate::build_fingerprint();
            let Ok(payload) = serde_json::from_slice::<Value>(&request.body) else {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_REQUEST",
                    "Expected JSON body.",
                    Some("Send: {\"fingerprint\":\"...\"} from /api/about."),
                    false,
                );
            };
            let provided = payload
                .get("fingerprint")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if provided.is_empty() || provided != expected {
                return write_api_error(
                    &mut stream,
                    "409 Conflict",
                    "FINGERPRINT_MISMATCH",
                    "Viewer fingerprint mismatch.",
                    Some("Reload /api/about and retry shutdown."),
                    false,
                );
            }
            shutdown.store(true, Ordering::Relaxed);
            let body = json!({ "ok": true }).to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                false,
            )
        }
        "/api/snapshot" => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let store = match stores.store_for(&request_storage_dir) {
                Ok(store) => store,
                Err(err) => {
                    return write_api_error(
                        &mut stream,
                        "503 Service Unavailable",
                        "PROJECT_UNAVAILABLE",
                        "Unable to open project store in read-only mode.",
                        Some(&format!("{err}")),
                        method == "HEAD",
                    );
                }
            };

            match snapshot::build_snapshot(store, &request_config, workspace_override.as_deref()) {
                Ok(payload) => {
                    let body = payload.to_string();
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
                Err(err) => {
                    let body = err.to_json().to_string();
                    write_response(
                        &mut stream,
                        err.status_line(),
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
            }
        }
        "/api/settings" if method == "GET" || method == "HEAD" => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let enabled = request_config
                .runner_autostart_enabled
                .as_ref()
                .map(|flag| flag.load(Ordering::Relaxed))
                .unwrap_or(false);
            let body = json!({
                "runner_autostart": {
                    "enabled": enabled,
                    "dry_run": request_config.runner_autostart_dry_run
                }
            })
            .to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
        "/api/settings/runner_autostart" if method == "POST" => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    false,
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    false,
                );
            }

            let Some(flag) = request_config.runner_autostart_enabled.as_ref() else {
                let body = json!({
                    "error": {
                        "code": "SETTINGS_UNSUPPORTED",
                        "message": "Runner autostart is not configurable in this server mode.",
                        "recovery": "Restart the server with viewer enabled and local autostart support."
                    }
                })
                .to_string();
                return write_response(
                    &mut stream,
                    "409 Conflict",
                    "application/json; charset=utf-8",
                    body.as_bytes(),
                    false,
                );
            };

            let desired = match parse_settings_bool(&request.body, "enabled") {
                Ok(value) => value,
                Err(payload) => {
                    let body = payload.to_string();
                    return write_response(
                        &mut stream,
                        "400 Bad Request",
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        false,
                    );
                }
            };
            flag.store(desired, Ordering::Relaxed);
            let body = json!({
                "ok": true,
                "runner_autostart": {
                    "enabled": flag.load(Ordering::Relaxed),
                    "dry_run": request_config.runner_autostart_dry_run
                }
            })
            .to_string();
            write_response(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                false,
            )
        }
        path if path.starts_with("/api/task/") => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let store = match stores.store_for(&request_storage_dir) {
                Ok(store) => store,
                Err(err) => {
                    return write_api_error(
                        &mut stream,
                        "503 Service Unavailable",
                        "PROJECT_UNAVAILABLE",
                        "Unable to open project store in read-only mode.",
                        Some(&format!("{err}")),
                        method == "HEAD",
                    );
                }
            };

            let task_id = path.trim_start_matches("/api/task/").trim();
            match detail::build_task_detail(
                store,
                &request_config,
                workspace_override.as_deref(),
                task_id,
                trace_cursor,
                notes_cursor,
            ) {
                Ok(payload) => {
                    let body = payload.to_string();
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
                Err(err) => {
                    let body = err.to_json().to_string();
                    write_response(
                        &mut stream,
                        err.status_line(),
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
            }
        }
        path if path.starts_with("/api/plan/") => {
            if project_param_invalid {
                return write_api_error(
                    &mut stream,
                    "400 Bad Request",
                    "INVALID_PROJECT",
                    "project: invalid project guard.",
                    Some("Use a value like repo:0123abcd… from /api/projects."),
                    method == "HEAD",
                );
            }
            if project_unknown {
                return write_api_error(
                    &mut stream,
                    "404 Not Found",
                    "UNKNOWN_PROJECT",
                    "Unknown project.",
                    Some("Pick one of the active projects returned by /api/projects."),
                    method == "HEAD",
                );
            }

            let store = match stores.store_for(&request_storage_dir) {
                Ok(store) => store,
                Err(err) => {
                    return write_api_error(
                        &mut stream,
                        "503 Service Unavailable",
                        "PROJECT_UNAVAILABLE",
                        "Unable to open project store in read-only mode.",
                        Some(&format!("{err}")),
                        method == "HEAD",
                    );
                }
            };

            let plan_id = path.trim_start_matches("/api/plan/").trim();
            match detail::build_plan_detail(
                store,
                &request_config,
                workspace_override.as_deref(),
                plan_id,
                trace_cursor,
                notes_cursor,
            ) {
                Ok(payload) => {
                    let body = payload.to_string();
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
                Err(err) => {
                    let body = err.to_json().to_string();
                    write_response(
                        &mut stream,
                        err.status_line(),
                        "application/json; charset=utf-8",
                        body.as_bytes(),
                        method == "HEAD",
                    )
                }
            }
        }
        _ => write_response(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not found.",
            method == "HEAD",
        ),
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpRequest>> {
    let mut buf = [0u8; 4096];
    let mut data = Vec::<u8>::new();
    loop {
        let read = match stream.read(&mut buf) {
            Ok(read) => read,
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(err) => return Err(err),
        };
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buf[..read]);
        if data.windows(4).any(|w| w == b"\r\n\r\n") || data.len() > 8192 {
            break;
        }
    }
    if data.is_empty() {
        return Ok(None);
    }

    let header_end = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
        .unwrap_or(data.len());
    let header_end = header_end.min(data.len());
    let header_bytes = &data[..header_end];
    let mut body = data[header_end..].to_vec();

    let header_text = String::from_utf8_lossy(header_bytes);
    let mut lines = header_text.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length: usize = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }

    const MAX_BODY_BYTES: usize = 16 * 1024;
    if content_length > MAX_BODY_BYTES {
        content_length = MAX_BODY_BYTES;
    }

    if content_length > body.len() {
        let mut remaining = content_length - body.len();
        while remaining > 0 {
            let read = match stream.read(&mut buf) {
                Ok(read) => read,
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(err) => return Err(err),
            };
            if read == 0 {
                break;
            }
            let take = read.min(remaining);
            body.extend_from_slice(&buf[..take]);
            remaining = remaining.saturating_sub(take);
        }
    } else {
        body.truncate(content_length);
    }

    Ok(Some(HttpRequest { method, path, body }))
}

fn normalize_path(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw.split('?').next().unwrap_or(raw);
    let raw = raw.trim();
    if raw.is_empty() {
        return "/".to_string();
    }
    if raw.len() > 256 || raw.contains("..") || raw.contains('\\') {
        return "/".to_string();
    }
    raw.to_string()
}

fn extract_query_param_raw(raw: &str, key: &str) -> Option<String> {
    let query = raw.split_once('?')?.1;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or("").trim();
        if name != key {
            continue;
        }
        let value = parts.next().unwrap_or("").trim();
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

fn decode_query_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return None;
    }

    let mut out: Vec<u8> = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'+' => {
                out.push(b' ');
                idx += 1;
            }
            b'%' if idx + 2 < bytes.len() => {
                let hi = bytes[idx + 1];
                let lo = bytes[idx + 2];
                let hex = |b: u8| match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                };
                let hi = hex(hi)?;
                let lo = hex(lo)?;
                out.push((hi << 4) | lo);
                idx += 3;
            }
            byte => {
                out.push(byte);
                idx += 1;
            }
        }
    }

    String::from_utf8(out).ok()
}

fn normalize_workspace_param(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn normalize_project_guard_param(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 96 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '_' | '-'))
    {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn parse_cursor_param(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Accept 0 as a sentinel (fetch nothing), but reject negative cursors.
    let parsed = trimmed.parse::<i64>().ok()?;
    if parsed < 0 {
        return None;
    }
    Some(parsed)
}

fn repo_root_from_cwd() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            break;
        }
    }
    cwd
}

fn is_repo_local_storage_dir(storage_dir: &Path) -> bool {
    let canonical =
        std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
    let Some(repo_root) = repo_root_from_storage_dir(&canonical) else {
        return false;
    };
    repo_root.join(".git").exists()
}

fn repo_root_from_storage_dir(storage_dir: &Path) -> Option<PathBuf> {
    let dir_name = storage_dir.file_name().and_then(|name| name.to_str())?;
    if dir_name != ".branchmind" {
        return None;
    }
    let mcp_dir = storage_dir.parent()?;
    if mcp_dir.file_name().and_then(|v| v.to_str()) != Some("mcp") {
        return None;
    }
    let agents_dir = mcp_dir.parent()?;
    if agents_dir.file_name().and_then(|v| v.to_str()) != Some(".agents") {
        return None;
    }
    agents_dir.parent().map(|p| p.to_path_buf())
}

fn default_workspace_from_root(root: &Path) -> String {
    let raw = root
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("workspace");
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn recommended_workspace_from_storage_dir(storage_dir: &Path) -> String {
    let canonical =
        std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
    let root = if is_repo_local_storage_dir(&canonical) {
        canonical
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(repo_root_from_cwd)
    } else {
        repo_root_from_cwd()
    };
    default_workspace_from_root(&root)
}

fn write_api_error(
    stream: &mut TcpStream,
    status: &str,
    code: &str,
    message: &str,
    recovery: Option<&str>,
    head_only: bool,
) -> std::io::Result<()> {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
            "recovery": recovery,
        }
    })
    .to_string();
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        body.as_bytes(),
        head_only,
    )
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let mut headers = String::new();
    headers.push_str("HTTP/1.1 ");
    headers.push_str(status);
    headers.push_str("\r\n");
    headers.push_str("Content-Type: ");
    headers.push_str(content_type);
    headers.push_str("\r\n");
    headers.push_str("Cache-Control: no-store\r\n");
    headers.push_str("X-Content-Type-Options: nosniff\r\n");
    headers.push_str("Content-Security-Policy: default-src 'self'; style-src 'self'; script-src 'self'; img-src 'self' data:;\r\n");
    headers.push_str("Content-Length: ");
    headers.push_str(&body.len().to_string());
    headers.push_str("\r\n\r\n");

    stream.write_all(headers.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    Ok(())
}

fn parse_settings_bool(body: &[u8], field: &str) -> Result<bool, Value> {
    let payload: Value = serde_json::from_slice(body).map_err(|_| {
        json!({
            "error": {
                "code": "INVALID_JSON",
                "message": "Request body must be valid JSON.",
                "recovery": "Send {\"enabled\": true|false}."
            }
        })
    })?;

    let value = payload
        .get(field)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| {
            json!({
                "error": {
                    "code": "INVALID_INPUT",
                    "message": format!("Missing or invalid boolean field: {field}."),
                    "recovery": "Send {\"enabled\": true|false}."
                }
            })
        })?;
    Ok(value)
}
