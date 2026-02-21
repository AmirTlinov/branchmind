#![forbid(unsafe_code)]

use crate::McpServer;
use crate::entry::framing::{parse_request, read_content_length_frame, write_content_length_json};
use bm_storage::SqliteStore;
use serde_json::{Value, json};
use std::io::{BufReader, BufWriter};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct DaemonConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) socket_path: PathBuf,
    pub(crate) hot_reload_enabled: bool,
    pub(crate) hot_reload_poll_ms: u64,
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
                let config = Arc::clone(&config);
                let counter = Arc::clone(&active_connections);
                counter.fetch_add(1, Ordering::SeqCst);
                thread::spawn(move || {
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

struct ConnGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

fn handle_connection(
    stream: UnixStream,
    config: Arc<DaemonConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = config.socket_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    let mut server = build_server(&config)?;

    loop {
        let Some(body) = read_content_length_frame(&mut reader, None)? else {
            break;
        };

        let response: Option<Value> = match parse_request(&body) {
            Ok(request) => {
                let expects_response = !matches!(request.id.as_ref(), None | Some(Value::Null));

                // Internal: allow the shared proxy to terminate a stale/unhealthy daemon so a fresh
                // build can take over the stable socket path.
                if request.method == "branchmind/daemon_shutdown" {
                    if expects_response {
                        let resp = crate::json_rpc_response(request.id, json!({ "ok": true }));
                        let _ = std::fs::remove_file(&config.socket_path);
                        write_content_length_json(&mut writer, &resp)?;
                    }
                    std::process::exit(0);
                }

                // Internal: allow the shared proxy to verify that a reused daemon matches the
                // current session's config (storage_dir). This prevents cross-project drift when
                // multiple repos share a runtime socket base directory.
                if request.method == "branchmind/daemon_info" {
                    if !expects_response {
                        None
                    } else {
                        let storage_dir = std::fs::canonicalize(&config.storage_dir)
                            .unwrap_or_else(|_| config.storage_dir.clone())
                            .to_string_lossy()
                            .to_string();
                        let argv0 = std::env::args_os()
                            .next()
                            .map(|v| v.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let exe_path = std::env::current_exe()
                            .ok()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        Some(crate::json_rpc_response(
                            request.id,
                            json!({
                                "compat_fingerprint": crate::build_compat_fingerprint(),
                                "fingerprint": crate::build_fingerprint(),
                                "build_time_ms": crate::binary_build_time_ms().unwrap_or(0),
                                "argv0": argv0,
                                "exe_path": exe_path,
                                "storage_dir": storage_dir,
                            }),
                        ))
                    }
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
    let store = SqliteStore::open(&config.storage_dir)?;
    Ok(McpServer::new(store))
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

    // When BM_GIT_SHA is unavailable (non-git builds), fall back to a cheap build-time heuristic.
    if crate::build_git_sha().is_none() {
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

    Ok(true)
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

#[allow(dead_code)]
fn socket_path_fits_unix_limit(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        const MAX_BYTES: usize = 100;
        path.as_os_str().as_bytes().len() < MAX_BYTES
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}
