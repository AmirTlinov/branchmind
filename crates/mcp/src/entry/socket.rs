#![forbid(unsafe_code)]

use crate::entry::framing::{parse_request, read_content_length_frame, write_content_length_json};
use crate::{DefaultAgentIdConfig, McpServer, Toolset};
use bm_storage::SqliteStore;
use serde_json::Value;
use std::io::{BufReader, BufWriter};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

#[derive(Clone)]
pub(crate) struct DaemonConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) toolset: Toolset,
    pub(crate) default_workspace: Option<String>,
    pub(crate) workspace_lock: bool,
    pub(crate) project_guard: Option<String>,
    pub(crate) default_agent_id_config: Option<DefaultAgentIdConfig>,
    pub(crate) socket_path: PathBuf,
}

pub(crate) fn run_socket_daemon(config: DaemonConfig) -> Result<(), Box<dyn std::error::Error>> {
    if UnixStream::connect(&config.socket_path).is_ok() {
        return Ok(());
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

    let config = Arc::new(config);

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let config = Arc::clone(&config);
        thread::spawn(move || {
            let _ = handle_connection(stream, config);
        });
    }

    Ok(())
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
            Ok(request) => server.handle(request),
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
        config.toolset,
        config.default_workspace.clone(),
        config.workspace_lock,
        config.project_guard.clone(),
        default_agent_id,
    ))
}
