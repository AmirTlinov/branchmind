#![forbid(unsafe_code)]

use crate::entry::framing::{
    TransportMode, detect_mode_from_first_line, read_content_length_frame, request_expects_response,
};
use crate::{DefaultAgentIdConfig, Toolset};
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

pub(crate) fn run_shared_proxy(config: SharedProxyConfig) -> Result<(), Box<dyn std::error::Error>>
{
    let stream = connect_or_spawn(&config)?;
    let mut daemon_reader = BufReader::new(stream.try_clone()?);
    let mut daemon_writer = BufWriter::new(stream);

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
                            &mut daemon_reader,
                            &mut daemon_writer,
                            &mut stdout,
                        )?;
                        continue;
                    }
                    TransportMode::ContentLength => {
                        let Some(body) = read_content_length_frame(&mut reader, Some(peek))?
                        else {
                            break;
                        };
                        handle_client_body(
                            body,
                            detected,
                            &mut daemon_reader,
                            &mut daemon_writer,
                            &mut stdout,
                        )?;
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
                    &mut daemon_reader,
                    &mut daemon_writer,
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
                let Some(body) = read_content_length_frame(&mut reader, Some(first_header))?
                else {
                    break;
                };
                handle_client_body(
                    body,
                    effective_mode,
                    &mut daemon_reader,
                    &mut daemon_writer,
                    &mut stdout,
                )?;
            }
        }
    }

    Ok(())
}

fn handle_client_body(
    body: Vec<u8>,
    mode: TransportMode,
    daemon_reader: &mut BufReader<UnixStream>,
    daemon_writer: &mut BufWriter<UnixStream>,
    stdout: &mut std::io::StdoutLock<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let expects_response = request_expects_response(&body);
    write_content_length_raw(daemon_writer, &body)?;

    if !expects_response {
        return Ok(());
    }

    let Some(resp_body) = read_content_length_frame(daemon_reader, None)? else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "daemon connection closed",
        )
        .into());
    };

    match mode {
        TransportMode::NewlineJson => write_newline_raw(stdout, &resp_body)?,
        TransportMode::ContentLength => write_content_length_raw(stdout, &resp_body)?,
    }

    Ok(())
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
