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
        "BRANCHMIND_RESPONSE_VERBOSITY",
        "BRANCHMIND_DX",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResponseVerbosity {
    Full,
    Compact,
}

impl ResponseVerbosity {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "compact" | "refs" => Some(Self::Compact),
            _ => None,
        }
    }

    pub(crate) fn parse(value: Option<&str>) -> Self {
        value
            .and_then(|v| Self::from_str(v.trim()))
            .unwrap_or(Self::Full)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Compact => "compact",
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

pub(crate) fn parse_response_verbosity() -> ResponseVerbosity {
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--response-verbosity";
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_RESPONSE_VERBOSITY").ok());
    ResponseVerbosity::parse(value.as_deref())
}

pub(crate) fn response_verbosity_explicit() -> bool {
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            return true;
        }
        saw_flag = arg.as_str() == "--response-verbosity";
    }

    std::env::var("BRANCHMIND_RESPONSE_VERBOSITY")
        .ok()
        .is_some_and(|raw| !raw.trim().is_empty())
}

pub(crate) fn parse_dx_mode() -> bool {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dx" => return true,
            "--no-dx" => return false,
            _ => {}
        }
    }

    if let Some(value) = parse_bool_env_override("BRANCHMIND_DX") {
        return value;
    }

    auto_mode_enabled()
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
        // Stability default: hot reload is opt-in to avoid transport drops in long-lived sessions.
        Err(_) => false,
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
    pub(crate) toolset: Toolset,
    pub(crate) response_verbosity: ResponseVerbosity,
    pub(crate) dx_mode: bool,
    pub(crate) ux_proof_v2_enabled: bool,
    pub(crate) knowledge_autolint_enabled: bool,
    pub(crate) note_promote_enabled: bool,
    pub(crate) default_workspace: Option<&'a str>,
    pub(crate) workspace_explicit: bool,
    pub(crate) workspace_lock: bool,
    pub(crate) workspace_allowlist: Option<&'a [String]>,
    pub(crate) project_guard: Option<&'a str>,
    pub(crate) default_agent_id: Option<&'a DefaultAgentIdConfig>,
}

pub(crate) fn socket_tag_for_config(cfg: SocketTagConfig<'_>) -> String {
    const FNV_OFFSET: u64 = 14695981039346656037;
    let mut hash = FNV_OFFSET;

    // Flagship stability:
    // Include a build-compat fingerprint in the socket tag so different `bm_mcp` builds
    // (e.g., Codex/Claude/Gemini shipping different copies/versions) don't fight over the
    // same shared daemon and cause cross-session transport drops.
    hash = fnv1a_kv(hash, "compat", cfg.compat_fingerprint);
    hash = fnv1a_kv(hash, "toolset", cfg.toolset.as_str());
    hash = fnv1a_kv(hash, "verbosity", cfg.response_verbosity.as_str());
    hash = fnv1a_kv(hash, "dx", if cfg.dx_mode { "1" } else { "0" });
    hash = fnv1a_kv(
        hash,
        "ux_proof_v2",
        if cfg.ux_proof_v2_enabled { "1" } else { "0" },
    );
    hash = fnv1a_kv(
        hash,
        "knowledge_autolint",
        if cfg.knowledge_autolint_enabled {
            "1"
        } else {
            "0"
        },
    );
    hash = fnv1a_kv(
        hash,
        "note_promote",
        if cfg.note_promote_enabled { "1" } else { "0" },
    );
    hash = fnv1a_kv(hash, "workspace", cfg.default_workspace.unwrap_or(""));
    hash = fnv1a_kv(
        hash,
        "workspace_explicit",
        if cfg.workspace_explicit { "1" } else { "0" },
    );
    hash = fnv1a_kv(
        hash,
        "workspace_lock",
        if cfg.workspace_lock { "1" } else { "0" },
    );

    let mut allowlist = cfg
        .workspace_allowlist
        .map(|list| list.to_vec())
        .unwrap_or_default();
    allowlist.sort();
    allowlist.dedup();
    if allowlist.is_empty() {
        hash = fnv1a_kv(hash, "allowlist", "none");
    } else {
        hash = fnv1a_update(hash, b"allowlist=");
        for item in allowlist {
            hash = fnv1a_update(hash, item.as_bytes());
            hash = fnv1a_update(hash, b",");
        }
    }

    hash = fnv1a_kv(hash, "project_guard", cfg.project_guard.unwrap_or(""));

    let agent_tag = match cfg.default_agent_id {
        Some(DefaultAgentIdConfig::Auto) => "auto".to_string(),
        Some(DefaultAgentIdConfig::Explicit(value)) => value.to_string(),
        None => "none".to_string(),
    };
    hash = fnv1a_kv(hash, "agent_id", agent_tag.as_str());

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

pub(crate) fn parse_ux_proof_v2_enabled() -> bool {
    parse_feature_enabled_with_default(
        true,
        "BRANCHMIND_UX_PROOF_V2",
        "--ux-proof-v2",
        "--no-ux-proof-v2",
    )
}

pub(crate) fn parse_knowledge_autolint_enabled() -> bool {
    parse_feature_enabled_with_default(
        true,
        "BRANCHMIND_KNOWLEDGE_AUTOLINT",
        "--knowledge-autolint",
        "--no-knowledge-autolint",
    )
}

pub(crate) fn parse_note_promote_enabled() -> bool {
    parse_feature_enabled_with_default(
        true,
        "BRANCHMIND_NOTE_PROMOTE",
        "--note-promote",
        "--no-note-promote",
    )
}

fn parse_feature_enabled_with_default(
    default_enabled: bool,
    env_key: &str,
    cli_on: &str,
    cli_off: &str,
) -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == cli_off {
            return false;
        }
        if arg.as_str() == cli_on {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_tag_is_stable_for_equivalent_config() {
        let allowlist_a = vec!["b".to_string(), "a".to_string()];
        let allowlist_b = vec!["a".to_string(), "b".to_string(), "b".to_string()];

        let tag_a = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "compat",
            toolset: Toolset::Daily,
            response_verbosity: ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
            default_workspace: Some("demo"),
            workspace_explicit: false,
            workspace_lock: true,
            workspace_allowlist: Some(&allowlist_a),
            project_guard: Some("repo:abc"),
            default_agent_id: Some(&DefaultAgentIdConfig::Auto),
        });
        let tag_b = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "compat",
            toolset: Toolset::Daily,
            response_verbosity: ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
            default_workspace: Some("demo"),
            workspace_explicit: false,
            workspace_lock: true,
            workspace_allowlist: Some(&allowlist_b),
            project_guard: Some("repo:abc"),
            default_agent_id: Some(&DefaultAgentIdConfig::Auto),
        });

        assert_eq!(tag_a, tag_b);
    }

    #[test]
    fn socket_tag_changes_when_config_changes() {
        let base = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "compat",
            toolset: Toolset::Daily,
            response_verbosity: ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
            default_workspace: Some("demo"),
            workspace_explicit: false,
            workspace_lock: true,
            workspace_allowlist: None,
            project_guard: Some("repo:abc"),
            default_agent_id: None,
        });
        let different = socket_tag_for_config(SocketTagConfig {
            compat_fingerprint: "compat",
            toolset: Toolset::Full,
            response_verbosity: ResponseVerbosity::Full,
            dx_mode: true,
            ux_proof_v2_enabled: true,
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
            default_workspace: Some("demo"),
            workspace_explicit: false,
            workspace_lock: true,
            workspace_allowlist: None,
            project_guard: Some("repo:abc"),
            default_agent_id: None,
        });
        assert_ne!(base, different);
    }
}
