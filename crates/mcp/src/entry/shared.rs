#![forbid(unsafe_code)]

use crate::entry::framing::{
    TransportMode, detect_mode_from_first_line, read_content_length_frame, request_expects_response,
};
use crate::json_rpc_error;
use crate::{DefaultAgentIdConfig, Toolset};
use serde_json::Value;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct SharedProxyConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) toolset: Toolset,
    pub(crate) default_workspace: Option<String>,
    pub(crate) workspace_lock: bool,
    pub(crate) project_guard: Option<String>,
    pub(crate) default_agent_id_config: Option<DefaultAgentIdConfig>,
    pub(crate) socket_path: PathBuf,
}

pub(crate) fn run_shared_proxy(
    config: SharedProxyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon: Option<DaemonPipe> = None;

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();

    let mut mode: Option<TransportMode> = None;

    loop {
        let effective_mode = match mode {
            Some(v) => v,
            None => {
                let mut peek = String::new();
                let read = reader.read_line(&mut peek)?;
                if read == 0 {
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
                        let raw = peek.trim();
                        if raw.is_empty() {
                            continue;
                        }
                        handle_client_body(
                            raw.as_bytes().to_vec(),
                            detected,
                            &mut daemon,
                            &config,
                            &mut stdout,
                        )?;
                        continue;
                    }
                    TransportMode::ContentLength => {
                        let Some(body) = read_content_length_frame(&mut reader, Some(peek))? else {
                            break;
                        };
                        handle_client_body(body, detected, &mut daemon, &config, &mut stdout)?;
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
                    break;
                }
                let raw = line.trim();
                if raw.is_empty() {
                    continue;
                }
                handle_client_body(
                    raw.as_bytes().to_vec(),
                    effective_mode,
                    &mut daemon,
                    &config,
                    &mut stdout,
                )?;
            }
            TransportMode::ContentLength => {
                let mut first_header = String::new();
                let read = reader.read_line(&mut first_header)?;
                if read == 0 {
                    break;
                }
                if first_header.trim().is_empty() {
                    continue;
                }
                let Some(body) = read_content_length_frame(&mut reader, Some(first_header))? else {
                    break;
                };
                handle_client_body(body, effective_mode, &mut daemon, &config, &mut stdout)?;
            }
        }
    }

    Ok(())
}

fn handle_client_body(
    body: Vec<u8>,
    mode: TransportMode,
    daemon: &mut Option<DaemonPipe>,
    config: &SharedProxyConfig,
    stdout: &mut std::io::StdoutLock<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let expects_response = request_expects_response(&body);
    let method = extract_request_method(&body);
    let reset_on_error = matches!(method.as_deref(), Some("initialize") | Some("ping"));
    let timeout = response_timeout_for_method(method.as_deref(), expects_response);
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
        Err(err) => {
            if expects_response {
                Some(build_transport_error_response(
                    &body,
                    err.to_string().as_str(),
                ))
            } else {
                None
            }
        }
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
        Some("initialize") | Some("ping") => Some(Duration::from_secs(2)),
        _ => Some(Duration::from_secs(30)),
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
        return Ok(stream);
    }

    spawn_daemon(config)?;

    let start = Instant::now();
    let deadline = Duration::from_secs(2);
    loop {
        if let Ok(stream) = UnixStream::connect(&config.socket_path) {
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

fn spawn_daemon(config: &SharedProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .arg("--daemon")
        .arg("--socket")
        .arg(&config.socket_path)
        .arg("--storage-dir")
        .arg(&config.storage_dir)
        .arg("--toolset")
        .arg(config.toolset.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(workspace) = &config.default_workspace {
        command.arg("--workspace").arg(workspace);
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

    command.spawn()?;
    Ok(())
}
