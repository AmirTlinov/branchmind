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
    let mut args = std::env::args().skip(1);
    let mut storage_dir: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--storage-dir"
            && let Some(value) = args.next()
        {
            storage_dir = Some(PathBuf::from(value));
        }
    }
    if let Some(dir) = storage_dir {
        return dir;
    }
    if auto_mode_enabled() {
        let root = default_repo_root();
        return root.join(".branchmind_rust");
    }
    PathBuf::from(".branchmind_rust")
}

pub(crate) fn parse_toolset() -> Toolset {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--toolset"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_TOOLSET").ok());
    if value.is_none() && auto_mode_enabled() {
        return Toolset::Daily;
    }
    Toolset::parse(value.as_deref())
}

pub(crate) fn parse_default_workspace() -> Option<String> {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--workspace"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_WORKSPACE").ok());
    if value.is_none() && auto_mode_enabled() {
        let root = default_repo_root();
        return Some(default_workspace_from_root(&root));
    }
    value
}

pub(crate) fn parse_workspace_lock() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--workspace-lock" {
            return true;
        }
    }
    if auto_mode_enabled() {
        return true;
    }
    parse_bool_env("BRANCHMIND_WORKSPACE_LOCK")
}

pub(crate) fn parse_project_guard() -> Option<String> {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--project-guard"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_PROJECT_GUARD").ok());
    if value.is_none() && auto_mode_enabled() {
        let root = default_repo_root();
        return Some(default_project_guard_from_root(&root));
    }
    value
}

#[derive(Clone, Debug)]
pub(crate) enum DefaultAgentIdConfig {
    Auto,
    Explicit(String),
}

pub(crate) fn parse_default_agent_id_config() -> Option<DefaultAgentIdConfig> {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--agent-id"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
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

pub(crate) fn parse_socket_path(storage_dir: &Path) -> PathBuf {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--socket"
            && let Some(value) = args.next()
        {
            cli = Some(PathBuf::from(value));
            break;
        }
    }

    cli.or_else(|| {
        std::env::var("BRANCHMIND_MCP_SOCKET")
            .ok()
            .map(PathBuf::from)
    })
    .unwrap_or_else(|| storage_dir.join("branchmind_mcp.sock"))
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
