#![forbid(unsafe_code)]

use crate::entry::framing::{
    TransportMode, detect_mode_from_first_line, parse_request, read_content_length_frame,
    request_expects_response,
};
use crate::json_rpc_error;
use crate::{DefaultAgentIdConfig, McpServer, ResponseVerbosity, Toolset};
use bm_storage::SqliteStore;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::io::AsFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct SharedProxyConfig {
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
    pub(crate) hot_reload_enabled: bool,
    pub(crate) hot_reload_poll_ms: u64,
    pub(crate) runner_autostart_dry_run: bool,
    pub(crate) runner_autostart_enabled_shared: Arc<AtomicBool>,
    pub(crate) runner_autostart_state_shared: Arc<Mutex<crate::RunnerAutostartState>>,
}

pub(crate) fn run_shared_proxy(
    config: SharedProxyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon: Option<DaemonPipe> = None;
    let mut local_server: Option<McpServer> = None;
    let mut session_log = crate::SessionLog::new(&config.storage_dir);

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();

    let hot_reload = crate::HotReload::start(
        config.hot_reload_enabled,
        Duration::from_millis(config.hot_reload_poll_ms),
    );

    let mut mode: Option<TransportMode> = None;

    loop {
        // Safe-point for hot reload: only when BufReader has no pre-fetched bytes.
        let _ = hot_reload.maybe_exec_if_requested_and_safe(reader.buffer().is_empty());
        // Avoid blocking indefinitely on stdin: poll with a timeout so hot reload can trigger even
        // when the MCP client is idle.
        if reader.buffer().is_empty()
            && !crate::entry::poll::wait_fd_readable(
                reader.get_ref().as_fd(),
                Duration::from_millis(100),
            )
        {
            continue;
        }

        let effective_mode = match mode {
            Some(v) => v,
            None => {
                let mut peek = String::new();
                let read = reader.read_line(&mut peek)?;
                if read == 0 {
                    session_log.note_exit("stdin_eof_before_mode");
                    break;
                }
                if let Some(detected) = detect_mode_from_first_line(&peek) {
                    mode = Some(detected);
                }
                if mode.is_none() {
                    continue;
                }

                let detected = mode.unwrap();
                match detected {
                    TransportMode::NewlineJson => {
                        session_log.note_mode("newline_json", &peek);
                        let raw = peek.trim();
                        if raw.is_empty() {
                            continue;
                        }
                        if let Err(err) = handle_client_body(
                            raw.as_bytes().to_vec(),
                            detected,
                            &mut daemon,
                            &mut local_server,
                            &config,
                            &mut stdout,
                            &mut session_log,
                        ) {
                            session_log.note_error(format!("{err}").as_str());
                            return Err(err);
                        }
                        continue;
                    }
                    TransportMode::ContentLength => {
                        session_log.note_mode("content_length", &peek);
                        let body = match read_content_length_frame(&mut reader, Some(peek)) {
                            Ok(Some(body)) => body,
                            Ok(None) => {
                                session_log.note_exit("stdin_eof_during_first_frame");
                                break;
                            }
                            Err(err) => {
                                session_log.note_error(format!("first_frame: {err}").as_str());
                                // Keep the proxy alive: invalid framing must not kill the transport.
                                mode = None;
                                continue;
                            }
                        };
                        if let Err(err) = handle_client_body(
                            body,
                            detected,
                            &mut daemon,
                            &mut local_server,
                            &config,
                            &mut stdout,
                            &mut session_log,
                        ) {
                            session_log.note_error(format!("{err}").as_str());
                            return Err(err);
                        }
                        continue;
                    }
                }
            }
        };

        match effective_mode {
            TransportMode::NewlineJson => {
                let mut line = String::new();
                let read = reader.read_line(&mut line)?;
                if read == 0 {
                    session_log.note_exit("stdin_eof");
                    break;
                }
                let raw = line.trim();
                if raw.is_empty() {
                    continue;
                }
                if let Err(err) = handle_client_body(
                    raw.as_bytes().to_vec(),
                    effective_mode,
                    &mut daemon,
                    &mut local_server,
                    &config,
                    &mut stdout,
                    &mut session_log,
                ) {
                    session_log.note_error(format!("{err}").as_str());
                    return Err(err);
                }
            }
            TransportMode::ContentLength => {
                let mut first_header = String::new();
                let read = reader.read_line(&mut first_header)?;
                if read == 0 {
                    session_log.note_exit("stdin_eof");
                    break;
                }
                if first_header.trim().is_empty() {
                    continue;
                }
                let body = match read_content_length_frame(&mut reader, Some(first_header)) {
                    Ok(Some(body)) => body,
                    Ok(None) => {
                        session_log.note_exit("stdin_eof_during_frame");
                        break;
                    }
                    Err(err) => {
                        session_log.note_error(format!("frame: {err}").as_str());
                        // Keep the proxy alive: invalid framing must not kill the transport.
                        mode = None;
                        continue;
                    }
                };
                if let Err(err) = handle_client_body(
                    body,
                    effective_mode,
                    &mut daemon,
                    &mut local_server,
                    &config,
                    &mut stdout,
                    &mut session_log,
                ) {
                    session_log.note_error(format!("{err}").as_str());
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

fn handle_client_body(
    body: Vec<u8>,
    mode: TransportMode,
    daemon: &mut Option<DaemonPipe>,
    local_server: &mut Option<McpServer>,
    config: &SharedProxyConfig,
    stdout: &mut std::io::StdoutLock<'_>,
    session_log: &mut crate::SessionLog,
) -> Result<(), Box<dyn std::error::Error>> {
    let expects_response = request_expects_response(&body);
    let method = extract_request_method(&body);
    session_log.note_method(method.as_deref().unwrap_or(""));

    // Fast-path: handle MCP handshake + introspection locally so Codex can reliably start the
    // server even if the shared daemon is stale/dead/unavailable. This avoids startup timeouts
    // and keeps "daemon recovery" an implementation detail behind a stable stdio transport.
    match try_handle_locally(&body, method.as_deref(), expects_response, config) {
        LocalHandling::NotHandled => {}
        LocalHandling::NoResponse => return Ok(()),
        LocalHandling::Response(resp_body) => {
            match mode {
                TransportMode::NewlineJson => write_newline_raw(stdout, &resp_body)?,
                TransportMode::ContentLength => write_content_length_raw(stdout, &resp_body)?,
            }
            return Ok(());
        }
    }

    let reset_on_error = matches!(
        method.as_deref(),
        Some("initialize") | Some("ping") | Some("tools/call")
    );
    let timeout = response_timeout_for_request(method.as_deref(), &body, expects_response);
    let mut resp_body = match forward_body_with_reconnect(
        daemon,
        config,
        &body,
        expects_response,
        timeout,
        reset_on_error,
    ) {
        Ok(Some(resp_body)) => Some(resp_body),
        Ok(None) => None,
        Err(err) => match try_handle_inprocess_fallback(&body, local_server, config) {
            Ok(Some(resp_body)) => Some(resp_body),
            Ok(None) => None,
            Err(fallback_err) => {
                if expects_response {
                    Some(build_transport_error_response(
                        &body,
                        format!("{err} (fallback: {fallback_err})").as_str(),
                    ))
                } else {
                    None
                }
            }
        },
    };

    if expects_response
        && resp_body.as_ref().and_then(|body| parse_error_code(body)) == Some(-32002)
        && matches!(method.as_deref(), Some("tools/call"))
    {
        *daemon = None;
        let _ = std::fs::remove_file(&config.socket_path);
        resp_body = match forward_body_with_reconnect(
            daemon,
            config,
            &body,
            expects_response,
            timeout,
            true,
        ) {
            Ok(Some(resp_body)) => Some(resp_body),
            Ok(None) => None,
            Err(err) => Some(build_transport_error_response(
                &body,
                err.to_string().as_str(),
            )),
        };
    }

    if let Some(resp_body) = resp_body {
        match mode {
            TransportMode::NewlineJson => write_newline_raw(stdout, &resp_body)?,
            TransportMode::ContentLength => write_content_length_raw(stdout, &resp_body)?,
        }
    }

    Ok(())
}

fn try_handle_inprocess_fallback(
    body: &[u8],
    local_server: &mut Option<McpServer>,
    config: &SharedProxyConfig,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let request = match parse_request(body) {
        Ok(request) => request,
        Err(err) => {
            return Ok(Some(serde_json::to_vec(&err)?));
        }
    };

    let server = ensure_local_server(local_server, config)?;
    let response: Option<Value> = server.handle(request);
    let Some(resp) = response else {
        return Ok(None);
    };
    Ok(Some(serde_json::to_vec(&resp)?))
}

fn ensure_local_server<'a>(
    local_server: &'a mut Option<McpServer>,
    config: &SharedProxyConfig,
) -> Result<&'a mut McpServer, Box<dyn std::error::Error>> {
    if local_server.is_none() {
        let mut store = SqliteStore::open(&config.storage_dir)?;
        let default_agent_id = match &config.default_agent_id_config {
            Some(DefaultAgentIdConfig::Explicit(id)) => Some(id.clone()),
            Some(DefaultAgentIdConfig::Auto) => Some(store.default_agent_id_auto_get_or_create()?),
            None => None,
        };
        *local_server = Some(McpServer::new(
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
        ));
    }

    Ok(local_server
        .as_mut()
        .expect("local server must exist after ensure_local_server"))
}

#[derive(Debug)]
enum LocalHandling {
    NotHandled,
    NoResponse,
    Response(Vec<u8>),
}

fn try_handle_locally(
    body: &[u8],
    method: Option<&str>,
    expects_response: bool,
    _config: &SharedProxyConfig,
) -> LocalHandling {
    let Some(method) = method else {
        return LocalHandling::NotHandled;
    };

    // Notifications never expect a response and should never require a daemon.
    // MCP client compatibility: some clients send `initialized` instead of `notifications/initialized`.
    if method == "notifications/initialized" || method == "initialized" {
        return LocalHandling::NoResponse;
    }

    if !expects_response {
        // Client notifications are never allowed to break the transport.
        // In shared mode we also avoid daemon startup work for notifications, since
        // we don't currently support any notification-driven side effects.
        return LocalHandling::NoResponse;
    }

    let id = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v.get("id").cloned());

    match method {
        "initialize" => {
            // Some clients are strict about the server echoing the negotiated protocol version.
            // In shared mode we short-circuit initialize locally, so we must reflect the
            // client’s declared version here as well.
            let protocol_version = serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|v| v.get("params").cloned())
                .and_then(|v| v.get("protocolVersion").cloned())
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| crate::MCP_VERSION.to_string());
            let resp = crate::json_rpc_response(
                id,
                json!({
                    "protocolVersion": protocol_version,
                    "serverInfo": {
                        "name": crate::SERVER_NAME,
                        "version": crate::build_fingerprint()
                    },
                    "capabilities": {
                        "tools": {},
                        "resources": {},
                        "prompts": {},
                        "logging": {}
                    }
                }),
            );
            match serde_json::to_vec(&resp) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "ping" => match serde_json::to_vec(&crate::json_rpc_response(id, json!({}))) {
            Ok(bytes) => LocalHandling::Response(bytes),
            Err(_) => LocalHandling::NotHandled,
        },
        "resources/list" => {
            match serde_json::to_vec(&crate::json_rpc_response(id, json!({ "resources": [] }))) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "resources/templates/list" => match serde_json::to_vec(&crate::json_rpc_response(
            id,
            json!({ "resourceTemplates": [] }),
        )) {
            Ok(bytes) => LocalHandling::Response(bytes),
            Err(_) => LocalHandling::NotHandled,
        },
        "resources/read" => {
            match serde_json::to_vec(&crate::json_rpc_response(id, json!({ "contents": [] }))) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "prompts/list" => {
            match serde_json::to_vec(&crate::json_rpc_response(id, json!({ "prompts": [] }))) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "prompts/get" => {
            match serde_json::to_vec(&crate::json_rpc_error(id, -32602, "Unknown prompt")) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "logging/setLevel" => match serde_json::to_vec(&crate::json_rpc_response(id, json!({}))) {
            Ok(bytes) => LocalHandling::Response(bytes),
            Err(_) => LocalHandling::NotHandled,
        },
        "roots/list" => {
            match serde_json::to_vec(&crate::json_rpc_response(id, json!({ "roots": [] }))) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        "tools/list" => {
            // v1: strict surface = 10, ignore toolset overrides from legacy clients.
            let resp = crate::json_rpc_response(
                id,
                json!({ "tools": crate::tools_v1::tool_definitions() }),
            );
            match serde_json::to_vec(&resp) {
                Ok(bytes) => LocalHandling::Response(bytes),
                Err(_) => LocalHandling::NotHandled,
            }
        }
        _ => LocalHandling::NotHandled,
    }
}

struct DaemonPipe {
    reader: BufReader<UnixStream>,
    writer: BufWriter<UnixStream>,
}

impl DaemonPipe {
    fn connect(config: &SharedProxyConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = connect_or_spawn(config)?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: BufWriter::new(stream),
        })
    }

    fn send(
        &mut self,
        body: &[u8],
        expects_response: bool,
        timeout: Option<Duration>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        if let Some(timeout) = timeout {
            let _ = self.reader.get_ref().set_read_timeout(Some(timeout));
            let _ = self.writer.get_ref().set_write_timeout(Some(timeout));
        } else {
            let _ = self.reader.get_ref().set_read_timeout(None);
            let _ = self.writer.get_ref().set_write_timeout(None);
        }
        write_content_length_raw(&mut self.writer, body)?;
        if !expects_response {
            return Ok(None);
        }
        let Some(resp_body) = read_content_length_frame(&mut self.reader, None)? else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "daemon connection closed",
            )
            .into());
        };
        Ok(Some(resp_body))
    }
}

fn forward_body_with_reconnect(
    daemon: &mut Option<DaemonPipe>,
    config: &SharedProxyConfig,
    body: &[u8],
    expects_response: bool,
    timeout: Option<Duration>,
    reset_on_error: bool,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    const MAX_ATTEMPTS: usize = 2;
    let mut forced_reset = false;
    for _ in 0..MAX_ATTEMPTS {
        if daemon.is_none() {
            match DaemonPipe::connect(config) {
                Ok(pipe) => {
                    *daemon = Some(pipe);
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }

        if let Some(pipe) = daemon.as_mut() {
            match pipe.send(body, expects_response, timeout) {
                Ok(resp) => return Ok(resp),
                Err(_) => {
                    *daemon = None;
                    if reset_on_error && !forced_reset {
                        forced_reset = true;
                        let _ = std::fs::remove_file(&config.socket_path);
                    }
                    continue;
                }
            }
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "daemon connection unavailable",
    )
    .into())
}

fn build_transport_error_response(body: &[u8], message: &str) -> Vec<u8> {
    let id = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v.get("id").cloned());
    let payload = json_rpc_error(id, -32000, message);
    serde_json::to_vec(&payload).unwrap_or_else(|_| {
        b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32000,\"message\":\"daemon unavailable\"}}".to_vec()
    })
}

fn parse_error_code(body: &[u8]) -> Option<i64> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    value
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_i64())
}

fn extract_request_method(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    value
        .get("method")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn response_timeout_for_method(method: Option<&str>, expects_response: bool) -> Option<Duration> {
    if !expects_response {
        return None;
    }
    match method {
        Some("initialize") | Some("ping") => Some(Duration::from_secs(5)),
        _ => Some(Duration::from_secs(30)),
    }
}

fn extract_tools_call_name(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let name = value
        .get("params")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())?;

    let canonical = name
        .strip_prefix("branchmind/")
        .or_else(|| name.strip_prefix("branchmind."))
        .unwrap_or(name);
    Some(canonical.to_string())
}

fn response_timeout_for_request(
    method: Option<&str>,
    body: &[u8],
    expects_response: bool,
) -> Option<Duration> {
    if !expects_response {
        return None;
    }
    match method {
        Some("initialize") | Some("ping") => Some(Duration::from_secs(5)),
        Some("tools/call") => match extract_tools_call_name(body).as_deref() {
            Some("status") => Some(Duration::from_secs(5)),
            Some("tasks_snapshot") => Some(Duration::from_secs(10)),
            _ => Some(Duration::from_secs(20)),
        },
        _ => response_timeout_for_method(method, expects_response),
    }
}

fn write_newline_raw<W: Write>(writer: &mut W, body: &[u8]) -> std::io::Result<()> {
    let text = std::str::from_utf8(body).map_err(|err| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("utf8: {err}"))
    })?;
    writeln!(writer, "{text}")?;
    writer.flush()?;
    Ok(())
}

fn write_content_length_raw<W: Write>(writer: &mut W, body: &[u8]) -> std::io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body)?;
    writer.flush()?;
    Ok(())
}

fn connect_or_spawn(config: &SharedProxyConfig) -> Result<UnixStream, Box<dyn std::error::Error>> {
    if let Ok(stream) = UnixStream::connect(&config.socket_path) {
        // Self-healing shared mode:
        // - When a long-lived daemon is reused across sessions, it is easy to accidentally
        //   keep talking to an older binary after a local rebuild.
        // - When multiple repos share a stable socket path, it is easy to accidentally
        //   keep talking to a daemon that was started for a different project guard/storage.
        //
        // We proactively probe daemon identity. If it doesn't match this proxy config,
        // we restart (best-effort, low-noise).
        match daemon_is_compatible(&stream, config) {
            Ok(true) => return Ok(stream),
            Ok(false) => {
                let _ = recover_daemon(Some(stream), config);
                return connect_with_deadline(&config.socket_path, Duration::from_secs(2));
            }
            // Flagship stability: never kill a shared daemon just because a probe timed out or
            // returned malformed data. Transient probe failures are common under load and should
            // not cause cross-session transport drops.
            Err(_) => return Ok(stream),
        }
    }

    spawn_daemon(config)?;
    let stream = connect_with_deadline(&config.socket_path, Duration::from_secs(2))?;
    match daemon_is_compatible(&stream, config) {
        Ok(true) => Ok(stream),
        Ok(false) => {
            let _ = recover_daemon(Some(stream), config);
            connect_with_deadline(&config.socket_path, Duration::from_secs(2))
        }
        // A freshly spawned daemon may still be warming up (opening SQLite, etc.). Probing it via
        // a short-timeout internal request is best-effort only; fail-open and let the first real
        // request establish readiness.
        Err(_) => Ok(stream),
    }
}

fn spawn_daemon(config: &SharedProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Resilience note:
    // - In shared mode, it's common to rebuild the binary while a long-lived proxy keeps running.
    // - On Unix this can make `current_exe()` resolve to a `(... (deleted))` path which cannot be spawned.
    // - Fallback to argv[0] (the configured path) to allow the proxy to self-heal.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        candidates.push(exe);
    }
    if let Some(argv0) = std::env::args_os().next() {
        let argv0 = PathBuf::from(argv0);
        if !candidates.iter().any(|p| p == &argv0) {
            candidates.push(argv0);
        }
    }
    // Always include a PATH-based fallback. This recovers cases where both `current_exe()` and
    // `argv[0]` become invalid (e.g. deleted binary path) but a stable `bm_mcp` is still available
    // on PATH (common in managed toolchains).
    let fallback = PathBuf::from("bm_mcp");
    if !candidates.iter().any(|p| p == &fallback) {
        candidates.push(fallback);
    }

    let mut last_err: Option<std::io::Error> = None;
    for exe in candidates {
        let mut command = Command::new(&exe);
        command
            .arg("--daemon")
            .arg("--socket")
            .arg(&config.socket_path)
            .arg("--storage-dir")
            .arg(&config.storage_dir)
            .arg("--toolset")
            .arg(config.toolset.as_str())
            .arg("--response-verbosity")
            .arg(config.response_verbosity.as_str())
            .arg(if config.dx_mode { "--dx" } else { "--no-dx" })
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Prevent process explosions: daemons spawned by shared proxies should self-terminate
        // after a short idle period when no sessions use them anymore.
        if std::env::var_os("BRANCHMIND_MCP_DAEMON_IDLE_EXIT_SECS").is_none() {
            command.env("BRANCHMIND_MCP_DAEMON_IDLE_EXIT_SECS", "120");
        }

        if config.workspace_explicit {
            if let Some(workspace) = &config.default_workspace {
                command.arg("--workspace").arg(workspace);
            }
            if !config.workspace_lock {
                command.env("BRANCHMIND_WORKSPACE_LOCK", "0");
            }
        }
        if let Some(allowlist) = &config.workspace_allowlist {
            command.env("BRANCHMIND_WORKSPACE_ALLOWLIST", allowlist.join(","));
        }
        if config.workspace_lock {
            command.arg("--workspace-lock");
        }
        if let Some(project_guard) = &config.project_guard {
            command.arg("--project-guard").arg(project_guard);
        }
        if let Some(agent_cfg) = &config.default_agent_id_config {
            match agent_cfg {
                DefaultAgentIdConfig::Auto => {
                    command.arg("--agent-id").arg("auto");
                }
                DefaultAgentIdConfig::Explicit(id) => {
                    command.arg("--agent-id").arg(id);
                }
            }
        }

        match command.spawn() {
            Ok(mut child) => {
                // Flagship stability: avoid accumulating `<defunct>` zombies when the daemon
                // self-terminates (idle exit) while a long-lived proxy is still running.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "spawn failed"))
        .into())
}

fn connect_with_deadline(
    socket_path: &PathBuf,
    deadline: Duration,
) -> Result<UnixStream, Box<dyn std::error::Error>> {
    let start = Instant::now();
    loop {
        if let Ok(stream) = UnixStream::connect(socket_path) {
            return Ok(stream);
        }
        if start.elapsed() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "daemon socket did not become ready",
    )
    .into())
}

fn daemon_is_compatible(
    stream: &UnixStream,
    config: &SharedProxyConfig,
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

    // If we have a git-backed compat fingerprint, treat it as code-identity and do not try to
    // “outsmart” it with file mtimes. Different agent CLIs may run the same build from different
    // paths (or copied binaries), and using mtimes here would cause needless daemon churn.
    //
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
    let body = serde_json::to_vec(request)?;
    write_content_length_raw(&mut writer, &body)?;
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

fn recover_daemon(
    existing: Option<UnixStream>,
    config: &SharedProxyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(stream) = existing.as_ref() {
        let _ = try_shutdown_daemon(stream);
    }
    // Best-effort: unlink the socket path so a fresh daemon can bind even if the old daemon
    // cannot be terminated (e.g. an older build that doesn't support shutdown).
    let _ = std::fs::remove_file(&config.socket_path);
    spawn_daemon(config)?;
    Ok(())
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
