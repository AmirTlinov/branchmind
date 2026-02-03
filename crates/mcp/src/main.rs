#![forbid(unsafe_code)]

mod entry;
mod handlers;
mod ops;
mod server;
mod support;
mod tools_v1;
mod viewer;

pub(crate) use support::*;

pub(crate) use bm_core::ids::WorkspaceId;
pub(crate) use bm_core::model::{ReasoningRef, TaskKind};
pub(crate) use bm_core::paths::StepPath;
use bm_storage::SqliteStore;
pub(crate) use bm_storage::StoreError;
use std::fmt::Write as _;
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

fn write_last_crash(storage_dir: &std::path::Path, kind: &str, detail: &str) {
    // Best-effort crash report to help debug MCP transport issues without logging request bodies.
    let _ = std::fs::create_dir_all(storage_dir);
    let path = storage_dir.join("branchmind_mcp_last_crash.txt");

    let mut out = String::new();
    let ts_ms = crate::support::now_ms_i64();
    let _ = writeln!(out, "ts={}", crate::support::ts_ms_to_rfc3339(ts_ms));
    let _ = writeln!(out, "pid={}", std::process::id());
    let _ = writeln!(out, "kind={kind}");
    let _ = writeln!(out, "build={}", crate::build_fingerprint());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let _ = writeln!(out, "cwd={}", cwd.to_string_lossy());
    let _ = writeln!(out, "args={:?}", std::env::args().collect::<Vec<_>>());
    let _ = writeln!(out, "detail={detail}");

    let _ = std::fs::write(path, out);
}

fn write_last_spawn(kind: &str) {
    // Best-effort spawn record for diagnosing "Transport closed" cases where the client never
    // establishes framing (and repo-local session logs might not be writable).
    //
    // This file is local-only and contains no request bodies.
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("branchmind_mcp");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("branchmind_mcp_last_spawn.txt");

    let mut out = String::new();
    let ts_ms = crate::support::now_ms_i64();
    let _ = writeln!(out, "ts={}", crate::support::ts_ms_to_rfc3339(ts_ms));
    let _ = writeln!(out, "pid={}", std::process::id());
    let _ = writeln!(out, "kind={kind}");
    let _ = writeln!(out, "build={}", crate::build_fingerprint());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let _ = writeln!(out, "cwd={}", cwd.to_string_lossy());
    let _ = writeln!(out, "args={:?}", std::env::args().collect::<Vec<_>>());

    let _ = std::fs::write(path, out);
}

fn install_crash_reporter(storage_dir: std::path::PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut detail = info.to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let _ = write!(&mut detail, "\nbacktrace:\n{backtrace}");
        write_last_crash(&storage_dir, "panic", &detail);
        default_hook(info);
    }));
}

pub(crate) struct McpServer {
    initialized: bool,
    store: SqliteStore,
    toolset: Toolset,
    response_verbosity: ResponseVerbosity,
    dx_mode: bool,
    ux_proof_v2_enabled: bool,
    knowledge_autolint_enabled: bool,
    note_promote_enabled: bool,
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
    response_verbosity: ResponseVerbosity,
    dx_mode: bool,
    ux_proof_v2_enabled: bool,
    knowledge_autolint_enabled: bool,
    note_promote_enabled: bool,
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

fn usage() -> &'static str {
    "bm_mcp — BranchMind MCP server (Rust, deterministic, stdio-first)\n\n\
USAGE:\n\
  bm_mcp [--storage-dir DIR] [--workspace WS] [--toolset daily|full|core]\n\
        [--shared|--daemon] [--socket PATH]\n\
\n\
FLAGS:\n\
  -h, --help       Print this help and exit\n\
  -V, --version    Print version/build and exit\n\
\n\
NOTES:\n\
  - Repo-local store default: <repo>/.agents/mcp/.branchmind/\n\
  - For full config/env vars, see README.md\n"
}

fn version_line() -> String {
    format!(
        "bm_mcp {SERVER_VERSION} build={}",
        crate::build_fingerprint()
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print!("{}", usage());
        return Ok(());
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-V" | "--version"))
    {
        println!("{}", version_line());
        return Ok(());
    }

    // Ensure the compatibility fingerprint is computed at process start.
    // This prevents a long-lived daemon from accidentally observing a replaced on-disk binary
    // and reporting a misleading "new" fingerprint while still running old code.
    let _ = build_fingerprint();
    let kind = if args.iter().any(|arg| arg.as_str() == "--daemon") {
        "daemon"
    } else if args.iter().any(|arg| arg.as_str() == "--shared") {
        "shared_proxy"
    } else {
        "stdio"
    };
    write_last_spawn(kind);

    let storage_dir = parse_storage_dir();
    install_crash_reporter(storage_dir.clone());
    // Always emit a small, bounded session record for debugging MCP transport issues.
    // This is written to the store directory (repo-local by default) and never to stdout/stderr.
    let _session_log = crate::SessionLog::new(&storage_dir);
    let storage_dir_for_errors = storage_dir.clone();
    let toolset = parse_toolset();
    let dx_mode = parse_dx_mode();
    let mut response_verbosity = parse_response_verbosity();
    if dx_mode && !response_verbosity_explicit() {
        response_verbosity = ResponseVerbosity::Compact;
    }
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
    let ux_proof_v2_enabled = parse_ux_proof_v2_enabled();
    let knowledge_autolint_enabled = parse_knowledge_autolint_enabled();
    let note_promote_enabled = parse_note_promote_enabled();
    let compat_fingerprint = crate::build_compat_fingerprint();
    let socket_tag = socket_tag_for_config(SocketTagConfig {
        compat_fingerprint: &compat_fingerprint,
        toolset,
        response_verbosity,
        dx_mode,
        ux_proof_v2_enabled,
        knowledge_autolint_enabled,
        note_promote_enabled,
        default_workspace: default_workspace.as_deref(),
        workspace_explicit: workspace_explicit.is_some(),
        workspace_lock,
        workspace_allowlist: workspace_allowlist.as_deref(),
        project_guard: project_guard.as_deref(),
        default_agent_id: default_agent_id_config.as_ref(),
    });
    let socket_path = parse_socket_path(&storage_dir, Some(&socket_tag));
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
                response_verbosity,
                dx_mode,
                ux_proof_v2_enabled,
                knowledge_autolint_enabled,
                note_promote_enabled,
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
            let result = entry::run_shared_proxy(config);
            if let Err(err) = &result {
                write_last_crash(&storage_dir_for_errors, "error", &format!("{err:?}"));
            }
            return result;
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
                response_verbosity,
                dx_mode,
                ux_proof_v2_enabled,
                knowledge_autolint_enabled,
                note_promote_enabled,
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
            let result = entry::run_socket_daemon(config);
            if let Err(err) = &result {
                write_last_crash(&storage_dir_for_errors, "error", &format!("{err:?}"));
            }
            return result;
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
            response_verbosity,
            dx_mode,
            ux_proof_v2_enabled,
            knowledge_autolint_enabled,
            note_promote_enabled,
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
    let result = entry::run_stdio(&mut server, hot_reload_enabled, hot_reload_poll_ms);
    if let Err(err) = &result {
        write_last_crash(&storage_dir_for_errors, "error", &format!("{err:?}"));
    }
    result
}
