#![forbid(unsafe_code)]

mod entry;
mod server;
mod support;
mod tools_v1;

pub(crate) use support::*;

pub(crate) use bm_core::ids::WorkspaceId;
use bm_storage::SqliteStore;
use std::fmt::Write as _;

// Protocol negotiation:
// Some MCP clients are strict about the server echoing a compatible protocol version.
// We keep this at the widely deployed baseline and remain forward-compatible in behavior.
const MCP_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "branchmind-rust-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn write_last_crash(storage_dir: &std::path::Path, kind: &str, detail: &str) {
    // Best-effort crash report to help debug MCP transport issues without logging request bodies.
    let _ = std::fs::create_dir_all(storage_dir);
    let path = storage_dir.join("branchmind_mcp_last_crash.txt");

    let mut out = String::new();
    let ts_ms = crate::now_ms_i64();
    let _ = writeln!(out, "ts={}", crate::ts_ms_to_rfc3339(ts_ms));
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
    let ts_ms = crate::now_ms_i64();
    let _ = writeln!(out, "ts={}", crate::ts_ms_to_rfc3339(ts_ms));
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
}

fn usage() -> &'static str {
    "bm_mcp — BranchMind MCP server (Rust, deterministic, reasoning-only)\n\n\
USAGE:\n\
  bm_mcp [--storage-dir DIR]\n\
        [--shared|--daemon|--shared-reset] [--socket PATH]\n\
        [--hot-reload|--no-hot-reload] [--hot-reload-poll-ms MS]\n\
\n\
FLAGS:\n\
  -h, --help       Print this help and exit\n\
  -V, --version    Print version/build and exit\n\
\n\
NOTES:\n\
  - Repo-local store default: <repo>/.agents/mcp/.branchmind/\n\
  - `--shared` is recommended for multi-session stability.\n"
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

    let storage_dir = parse_storage_dir();
    install_crash_reporter(storage_dir.clone());
    // Always emit a small, bounded session record for debugging MCP transport issues.
    // This is written to the store directory (repo-local by default) and never to stdout/stderr.
    let _session_log = crate::SessionLog::new(&storage_dir);
    let storage_dir_for_errors = storage_dir.clone();

    let compat_fingerprint = crate::build_compat_fingerprint();
    let socket_tag = socket_tag_for_config(SocketTagConfig {
        compat_fingerprint: &compat_fingerprint,
        storage_dir: &storage_dir,
    });
    let socket_path = parse_socket_path(&storage_dir, Some(&socket_tag));

    let shared_reset_mode = parse_shared_reset_mode();
    let shared_mode = parse_shared_mode();
    let daemon_mode = parse_daemon_mode();
    let hot_reload_enabled = parse_hot_reload_enabled();
    let hot_reload_poll_ms = parse_hot_reload_poll_ms();

    let kind = if shared_reset_mode {
        "shared_reset"
    } else if daemon_mode {
        "daemon"
    } else if shared_mode {
        "shared_proxy"
    } else {
        "stdio"
    };
    write_last_spawn(kind);

    if shared_reset_mode {
        #[cfg(unix)]
        {
            let config = entry::SharedProxyConfig {
                storage_dir,
                socket_path,
                socket_tag: Some(socket_tag),
                shared_reset_mode: true,
                hot_reload_enabled,
                hot_reload_poll_ms,
            };
            let result = entry::run_shared_proxy(config);
            if let Err(err) = &result {
                write_last_crash(&storage_dir_for_errors, "error", &format!("{err:?}"));
            }
            return result;
        }

        #[cfg(not(unix))]
        {
            return Err("shared reset mode is only supported on unix targets".into());
        }
    }

    if shared_mode {
        #[cfg(unix)]
        {
            let config = entry::SharedProxyConfig {
                storage_dir,
                socket_path,
                socket_tag: Some(socket_tag),
                shared_reset_mode: false,
                hot_reload_enabled,
                hot_reload_poll_ms,
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
            let config = entry::DaemonConfig {
                storage_dir,
                socket_path,
                hot_reload_enabled,
                hot_reload_poll_ms,
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

    let store = SqliteStore::open(&storage_dir)?;
    let mut server = McpServer::new(store);
    let result = entry::run_stdio(&mut server, hot_reload_enabled, hot_reload_poll_ms);
    if let Err(err) = &result {
        write_last_crash(&storage_dir_for_errors, "error", &format!("{err:?}"));
    }
    result
}
