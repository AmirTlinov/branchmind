#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

fn auto_mode_enabled() -> bool {
    if !auto_mode_enabled_for_args(std::env::args()) {
        return false;
    }

    // Auto-mode should only kick in for a totally default local DX launch.
    // If the user set any relevant env var, treat it as intentional configuration.
    let keys = [
        "BRANCHMIND_MCP_SHARED",
        "BRANCHMIND_MCP_DAEMON",
        "BRANCHMIND_MCP_SOCKET",
        "BRANCHMIND_HOT_RELOAD",
        "BRANCHMIND_HOT_RELOAD_POLL_MS",
    ];
    keys.iter().all(|key| std::env::var_os(key).is_none())
}

fn auto_mode_enabled_for_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // Auto-mode is enabled for:
    // - zero-arg runs (`bm_mcp`)
    // - shared-reset only (`bm_mcp --shared-reset`)
    //
    // Any additional flags mean the user is explicitly configuring the process.
    args.into_iter()
        .skip(1)
        .all(|arg| arg.as_ref() == "--shared-reset")
}

fn default_repo_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = cwd.clone();
    loop {
        let git = current.join(".git");
        if git.exists() {
            return current;
        }
        if !current.pop() {
            break;
        }
    }
    cwd
}

pub(crate) fn parse_storage_dir() -> PathBuf {
    let mut storage_dir: Option<PathBuf> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            storage_dir = Some(PathBuf::from(arg));
            saw_flag = false;
            continue;
        }
        saw_flag = arg.as_str() == "--storage-dir";
    }
    if let Some(dir) = storage_dir {
        return dir;
    }

    // Flagship DX: keep the store repo-local when running inside a git repo.
    // This is deterministic and avoids requiring `--storage-dir` in typical MCP client setups.
    let root = default_repo_root();
    root.join(".agents").join("mcp").join(".branchmind")
}

pub(crate) fn parse_shared_reset_mode() -> bool {
    std::env::args()
        .skip(1)
        .any(|arg| arg.as_str() == "--shared-reset")
}

pub(crate) fn parse_shared_mode() -> bool {
    if auto_mode_enabled() {
        return true;
    }
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--shared" {
            return true;
        }
    }
    parse_bool_env("BRANCHMIND_MCP_SHARED")
}

pub(crate) fn parse_daemon_mode() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--daemon" {
            return true;
        }
    }
    parse_bool_env("BRANCHMIND_MCP_DAEMON")
}

pub(crate) fn parse_hot_reload_enabled() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--no-hot-reload" {
            return false;
        }
        if arg.as_str() == "--hot-reload" {
            return true;
        }
    }

    match std::env::var("BRANCHMIND_HOT_RELOAD") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        // DX default:
        // - In shared proxy mode, hot reload is enabled by default to prevent “stale daemon”
        //   footguns after local rebuilds.
        // - In daemon mode and plain stdio, keep it opt-in.
        Err(_) => parse_shared_mode(),
    }
}

pub(crate) fn parse_hot_reload_poll_ms() -> u64 {
    // DX default: low-frequency polling to stay cheap but responsive in dev.
    const DEFAULT_POLL_MS: u64 = 1000;

    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            return arg
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_POLL_MS);
        }
        saw_flag = arg.as_str() == "--hot-reload-poll-ms";
    }

    std::env::var("BRANCHMIND_HOT_RELOAD_POLL_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_POLL_MS)
}

pub(crate) fn parse_socket_path(storage_dir: &Path, socket_tag: Option<&str>) -> PathBuf {
    let mut cli: Option<PathBuf> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(PathBuf::from(arg));
            break;
        }
        saw_flag = arg.as_str() == "--socket";
    }

    cli.or_else(|| {
        std::env::var("BRANCHMIND_MCP_SOCKET")
            .ok()
            .map(PathBuf::from)
    })
    .unwrap_or_else(|| {
        let filename = default_socket_filename(socket_tag);
        let candidate = storage_dir.join(&filename);
        if socket_path_fits_unix_limit(&candidate) {
            return candidate;
        }

        // Fallback: some repo roots are long enough that a tagged socket path exceeds the Unix
        // domain socket limit (SUN_LEN). Prefer a short, user-scoped runtime directory.
        let base = default_socket_base_dir();
        let dir = base.join("branchmind_mcp");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(filename)
    })
}

pub(crate) struct SocketTagConfig<'a> {
    pub(crate) compat_fingerprint: &'a str,
    pub(crate) storage_dir: &'a Path,
}

pub(crate) fn socket_tag_for_config(cfg: SocketTagConfig<'_>) -> String {
    const FNV_OFFSET: u64 = 14695981039346656037;
    let mut hash = FNV_OFFSET;

    // Flagship stability: include a build-compat fingerprint so different `bm_mcp` builds don't
    // fight over the same shared daemon.
    hash = fnv1a_kv(hash, "compat", cfg.compat_fingerprint);

    // Cross-project isolation: include the canonical storage dir path so that when we fall back
    // to a short runtime directory, different repos still get distinct socket filenames.
    let storage_dir = std::fs::canonicalize(cfg.storage_dir)
        .unwrap_or_else(|_| cfg.storage_dir.to_path_buf())
        .to_string_lossy()
        .to_string();
    hash = fnv1a_kv(hash, "storage_dir", storage_dir.as_str());

    format!("cfg.{hash:016x}")
}

fn default_socket_filename(socket_tag: Option<&str>) -> String {
    match socket_tag {
        // Keep the filename short: some projects live under long paths, and Unix domain sockets
        // have a small max path length. The tag already encodes config isolation.
        Some(tag) if !tag.trim().is_empty() => format!("bm.{tag}.sock"),
        _ => "branchmind_mcp.sock".to_string(),
    }
}

fn default_socket_base_dir() -> PathBuf {
    // Prefer per-user runtime dirs when available (short, auto-cleaned by OS).
    if let Ok(raw) = std::env::var("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            return path;
        }
    }
    std::env::temp_dir()
}

fn socket_path_fits_unix_limit(path: &Path) -> bool {
    // Unix domain sockets typically cap sun_path at 108 bytes including the trailing nul.
    // Use a conservative bound to avoid runtime errors like:
    //   InvalidInput: "path must be shorter than SUN_LEN"
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

fn fnv1a_kv(mut hash: u64, key: &str, value: &str) -> u64 {
    hash = fnv1a_update(hash, key.as_bytes());
    hash = fnv1a_update(hash, b"=");
    hash = fnv1a_update(hash, value.as_bytes());
    hash = fnv1a_update(hash, b";");
    hash
}

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 1099511628211;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn parse_bool_env(key: &str) -> bool {
    std::env::var(key).ok().is_some_and(|raw| {
        matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_reset_mode_parser_detects_flag() {
        assert!(parse_shared_reset_mode_from_args([
            "bm_mcp",
            "--shared-reset"
        ]));
    }

    #[test]
    fn shared_reset_mode_parser_ignores_other_flags() {
        assert!(!parse_shared_reset_mode_from_args(["bm_mcp", "--shared"]));
    }

    #[test]
    fn auto_mode_args_enabled_for_zero_arg_start() {
        assert!(auto_mode_enabled_for_args(["bm_mcp"]));
    }

    #[test]
    fn auto_mode_args_enabled_for_shared_reset_only() {
        assert!(auto_mode_enabled_for_args(["bm_mcp", "--shared-reset"]));
    }

    #[test]
    fn auto_mode_args_disabled_when_shared_reset_has_extra_flags() {
        assert!(!auto_mode_enabled_for_args([
            "bm_mcp",
            "--shared-reset",
            "--socket",
            "/tmp/sock"
        ]));
    }

    #[test]
    fn socket_tag_is_stable_for_equivalent_config() {
        let a = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "v1",
            storage_dir: Path::new("/tmp/a"),
        });
        let b = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "v1",
            storage_dir: Path::new("/tmp/a"),
        });
        assert_eq!(a, b);
    }

    #[test]
    fn socket_tag_changes_when_config_changes() {
        let base = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "v1",
            storage_dir: Path::new("/tmp/a"),
        });
        let different_storage = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "v1",
            storage_dir: Path::new("/tmp/b"),
        });
        assert_ne!(base, different_storage);

        let different_compat = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "v2",
            storage_dir: Path::new("/tmp/a"),
        });
        assert_ne!(base, different_compat);
    }

    fn parse_shared_reset_mode_from_args<I, S>(args: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        args.into_iter()
            .skip(1)
            .any(|arg| arg.as_ref() == "--shared-reset")
    }
}
