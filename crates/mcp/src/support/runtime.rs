#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

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
    storage_dir.unwrap_or_else(|| PathBuf::from(".branchmind_rust"))
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

    cli.or_else(|| std::env::var("BRANCHMIND_WORKSPACE").ok())
}

pub(crate) fn parse_workspace_lock() -> bool {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg.as_str() == "--workspace-lock" {
            return true;
        }
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

    cli.or_else(|| std::env::var("BRANCHMIND_PROJECT_GUARD").ok())
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg.as_str() == "--shared" {
            return true;
        }
    }
    parse_bool_env("BRANCHMIND_MCP_SHARED")
}

pub(crate) fn parse_daemon_mode() -> bool {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
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

    cli.or_else(|| std::env::var("BRANCHMIND_MCP_SOCKET").ok().map(PathBuf::from))
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
