#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

fn auto_mode_enabled() -> bool {
    if std::env::args().len() > 1 {
        return false;
    }
    let keys = [
        "BRANCHMIND_MCP_SHARED",
        "BRANCHMIND_MCP_DAEMON",
        "BRANCHMIND_MCP_SOCKET",
        "BRANCHMIND_WORKSPACE",
        "BRANCHMIND_WORKSPACE_ALLOWLIST",
        "BRANCHMIND_TOOLSET",
        "BRANCHMIND_AGENT_ID",
        "BRANCHMIND_WORKSPACE_LOCK",
        "BRANCHMIND_PROJECT_GUARD",
    ];
    keys.iter().all(|key| std::env::var_os(key).is_none())
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

fn default_project_guard_from_root(root: &Path) -> String {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical_str = canonical.to_string_lossy();
    let bytes = canonical_str.as_bytes();
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("repo:{hash:016x}")
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

fn default_project_guard_root_from_storage_dir(storage_dir: &Path) -> PathBuf {
    let canonical =
        std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
    if let Some(repo_root) = repo_root_from_storage_dir(&canonical)
        && is_repo_local_storage_dir(&canonical)
    {
        return repo_root;
    }
    default_repo_root()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Toolset {
    Full,
    Daily,
    Core,
}

impl Toolset {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "daily" | "dx" => Some(Self::Daily),
            "core" | "minimal" => Some(Self::Core),
            _ => None,
        }
    }

    pub(crate) fn parse(value: Option<&str>) -> Self {
        value.and_then(Self::from_str).unwrap_or(Self::Full)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Daily => "daily",
            Self::Core => "core",
        }
    }
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

pub(crate) fn parse_toolset() -> Toolset {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--toolset";
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_TOOLSET").ok());
    // Flagship DX: default to the small portal-first toolset unless explicitly overridden.
    if value.is_none() {
        return Toolset::Daily;
    }
    Toolset::parse(value.as_deref())
}

pub(crate) fn parse_workspace_explicit() -> Option<String> {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--workspace";
    }

    cli.or_else(|| std::env::var("BRANCHMIND_WORKSPACE").ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn parse_workspace_allowlist() -> Option<Vec<String>> {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--workspace-allowlist";
    }

    let raw = cli.or_else(|| std::env::var("BRANCHMIND_WORKSPACE_ALLOWLIST").ok())?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in raw.split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace()) {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

pub(crate) fn parse_default_workspace(
    explicit: Option<&str>,
    allowlist: Option<&[String]>,
) -> Option<String> {
    if let Some(value) = explicit {
        return Some(value.to_string());
    }

    // Flagship DX: pick a stable default workspace derived from the repo root.
    // This keeps agents from having to repeat `workspace=...` on every call.
    let root = default_repo_root();
    let derived = default_workspace_from_root(&root);
    match allowlist {
        Some(list) if !list.is_empty() => {
            if list.iter().any(|item| item == &derived) {
                Some(derived)
            } else {
                list.first().cloned()
            }
        }
        _ => Some(derived),
    }
}

pub(crate) fn parse_workspace_lock(explicit_workspace: bool, allowlist_present: bool) -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--workspace-lock" {
            return true;
        }
    }
    if let Some(value) = parse_bool_env_override("BRANCHMIND_WORKSPACE_LOCK") {
        return value;
    }
    if allowlist_present {
        return false;
    }
    if explicit_workspace {
        return true;
    }
    if auto_mode_enabled() {
        return true;
    }
    false
}

pub(crate) fn parse_project_guard_explicit() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--project-guard" {
            return true;
        }
    }
    match std::env::var("BRANCHMIND_PROJECT_GUARD") {
        Ok(value) => !value.trim().is_empty(),
        Err(_) => false,
    }
}

pub(crate) fn parse_project_guard(storage_dir: &Path) -> Option<String> {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--project-guard";
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_PROJECT_GUARD").ok());
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if value.is_some() {
        return value;
    }

    let root = default_project_guard_root_from_storage_dir(storage_dir);
    Some(default_project_guard_from_root(&root))
}

pub(crate) fn parse_project_guard_rebind_enabled(storage_dir: &Path) -> bool {
    if parse_project_guard_explicit() {
        return false;
    }
    is_repo_local_storage_dir(storage_dir)
}

#[derive(Clone, Debug)]
pub(crate) enum DefaultAgentIdConfig {
    Auto,
    Explicit(String),
}

pub(crate) fn parse_default_agent_id_config() -> Option<DefaultAgentIdConfig> {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--agent-id";
    }

    if cli.is_none() && std::env::var("BRANCHMIND_AGENT_ID").ok().is_none() && auto_mode_enabled() {
        return Some(DefaultAgentIdConfig::Auto);
    }

    let raw = cli.or_else(|| std::env::var("BRANCHMIND_AGENT_ID").ok())?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.eq_ignore_ascii_case("auto") {
        return Some(DefaultAgentIdConfig::Auto);
    }
    normalize_agent_id(raw).map(DefaultAgentIdConfig::Explicit)
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
        // DX default: enable hot reload in session modes so Codex users don't have to restart
        // the MCP server manually after rebuilding `bm_mcp`. Daemons remain opt-in.
        Err(_) => !parse_daemon_mode(),
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

pub(crate) fn parse_socket_path(storage_dir: &Path) -> PathBuf {
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
    .unwrap_or_else(|| storage_dir.join("branchmind_mcp.sock"))
}

pub(crate) fn parse_viewer_enabled() -> bool {
    parse_viewer_enabled_with_default(true, "BRANCHMIND_VIEWER")
}

pub(crate) fn parse_viewer_enabled_daemon() -> bool {
    // Daemons are long-lived by design; a local HTTP viewer would otherwise persist beyond the
    // calling session. Keep the daemon viewer opt-in.
    parse_viewer_enabled_with_default(false, "BRANCHMIND_VIEWER_DAEMON")
}

fn parse_viewer_enabled_with_default(default_enabled: bool, env_key: &str) -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--no-viewer" {
            return false;
        }
        if arg.as_str() == "--viewer" {
            return true;
        }
    }
    match std::env::var(env_key) {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default_enabled,
    }
}

pub(crate) fn parse_viewer_port() -> u16 {
    const DEFAULT_VIEWER_PORT: u16 = 7331;
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--viewer-port";
    }

    let raw = cli.or_else(|| std::env::var("BRANCHMIND_VIEWER_PORT").ok());
    let Some(raw) = raw else {
        return DEFAULT_VIEWER_PORT;
    };
    let parsed = raw.trim().parse::<u16>().ok();
    match parsed {
        Some(0) | None => DEFAULT_VIEWER_PORT,
        Some(value) => value,
    }
}

pub(crate) fn parse_runner_autostart_override() -> Option<bool> {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--no-runner-autostart" {
            return Some(false);
        }
        if arg.as_str() == "--runner-autostart" {
            return Some(true);
        }
    }
    match std::env::var("BRANCHMIND_RUNNER_AUTOSTART") {
        Ok(raw) => {
            let v = matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
            Some(v)
        }
        Err(_) => None,
    }
}

pub(crate) fn parse_runner_autostart_dry_run() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--runner-autostart-dry-run" {
            return true;
        }
    }
    parse_bool_env("BRANCHMIND_RUNNER_AUTOSTART_DRY_RUN")
}

fn parse_bool_env_override(key: &str) -> Option<bool> {
    let Ok(value) = std::env::var(key) else {
        return None;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_bool_env(key: &str) -> bool {
    let Ok(value) = std::env::var(key) else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn normalize_agent_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() > 64 {
        return None;
    }
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    for ch in chars {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            continue;
        }
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}
