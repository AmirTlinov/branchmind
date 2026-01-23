#![forbid(unsafe_code)]

use crate::now_ms_i64;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const CATALOG_DIR_ENV: &str = "BRANCHMIND_VIEWER_CATALOG_DIR";
const REGISTRY_DIR_ENV: &str = "BRANCHMIND_VIEWER_REGISTRY_DIR";
const SCAN_ROOTS_ENV: &str = "BRANCHMIND_VIEWER_SCAN_ROOTS";
const DEFAULT_DIR_NAME: &str = "branchmind_viewer";
const DEFAULT_CATALOG_SUBDIR: &str = "catalog";
const DEFAULT_SCAN_MAX_DEPTH: usize = 6;
const MAX_SCAN_STORES: usize = 2_000;
const STORE_DB_FILENAME: &str = "branchmind_rust.db";
const STORE_DIRNAME: &str = ".branchmind_rust";
const STALE_HEARTBEAT_MS: i64 = 30_000;

#[derive(Clone, Debug)]
pub(crate) struct PresenceConfig {
    pub(crate) storage_dir: PathBuf,
    pub(crate) project_guard: Option<String>,
    pub(crate) workspace_default: Option<String>,
    pub(crate) workspace_recommended: Option<String>,
    pub(crate) mode: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RegistryEntry {
    project_guard: String,
    label: String,
    storage_dir: String,
    workspace_default: Option<String>,
    workspace_recommended: Option<String>,
    updated_at_ms: i64,
    pid: u32,
    mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ProjectInfo {
    pub(crate) project_guard: String,
    pub(crate) label: String,
    pub(crate) storage_dir: PathBuf,
    pub(crate) workspace_default: Option<String>,
    pub(crate) workspace_recommended: Option<String>,
    pub(crate) updated_at_ms: i64,
    pub(crate) stale: bool,
    pub(crate) store_present: bool,
    pub(crate) is_temp: bool,
}

pub(crate) fn record_catalog_entry(config: PresenceConfig) {
    if registry_override_active() {
        return;
    }
    let Some(project_guard) = config.project_guard.as_deref() else {
        return;
    };
    let Some(dir) = catalog_dir_for_write() else {
        return;
    };

    let label = config
        .workspace_recommended
        .as_deref()
        .unwrap_or("project")
        .trim()
        .to_string();
    let storage_dir_str = std::fs::canonicalize(&config.storage_dir)
        .unwrap_or_else(|_| config.storage_dir.clone())
        .to_string_lossy()
        .to_string();

    let entry = RegistryEntry {
        project_guard: project_guard.to_string(),
        label,
        storage_dir: storage_dir_str,
        workspace_default: config.workspace_default.clone(),
        workspace_recommended: config.workspace_recommended.clone(),
        updated_at_ms: now_ms_i64(),
        pid: std::process::id(),
        mode: config.mode.to_string(),
    };

    let path = entry_path(&dir, project_guard);
    let _ = write_atomic_json(&path, &entry);
}

pub(crate) fn start_presence_writer(config: PresenceConfig) {
    let Some(project_guard) = config.project_guard.as_deref() else {
        return;
    };
    let label = config
        .workspace_recommended
        .as_deref()
        .unwrap_or("project")
        .trim()
        .to_string();

    let Some(dir) = registry_dir_for_write() else {
        return;
    };
    record_catalog_entry(config.clone());
    let path = entry_path(&dir, project_guard);
    let catalog_path = catalog_dir_for_write().map(|dir| entry_path(&dir, project_guard));
    let storage_dir_str = std::fs::canonicalize(&config.storage_dir)
        .unwrap_or_else(|_| config.storage_dir.clone())
        .to_string_lossy()
        .to_string();
    let workspace_default = config.workspace_default.clone();
    let workspace_recommended = config.workspace_recommended.clone();
    let mode = config.mode.to_string();
    let project_guard = project_guard.to_string();
    let pid = std::process::id();

    std::thread::spawn(move || {
        let mut entry = RegistryEntry {
            project_guard,
            label,
            storage_dir: storage_dir_str,
            workspace_default,
            workspace_recommended,
            updated_at_ms: now_ms_i64(),
            pid,
            mode,
        };

        let mut last_catalog_write_ms: i64 = 0;
        loop {
            entry.updated_at_ms = now_ms_i64();
            let _ = write_atomic_json(&path, &entry);
            if let Some(catalog_path) = catalog_path.as_ref() {
                // Keep a durable record of projects for multi-project browsing in the viewer.
                // Avoid excessive churn: update the catalog at a lower cadence than the live heartbeat.
                if last_catalog_write_ms == 0
                    || entry.updated_at_ms.saturating_sub(last_catalog_write_ms) >= 10_000
                {
                    let _ = write_atomic_json(catalog_path, &entry);
                    last_catalog_write_ms = entry.updated_at_ms;
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

pub(crate) fn list_projects() -> Vec<ProjectInfo> {
    let now_ms = now_ms_i64();
    let mut out: std::collections::HashMap<String, ProjectInfo> = std::collections::HashMap::new();

    for dir in registry_dirs_for_read() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let Some(entry) = read_entry(&path) else {
                continue;
            };
            let updated_at_ms = entry.updated_at_ms;
            let storage_dir = PathBuf::from(entry.storage_dir);
            let store_present = storage_dir.join(STORE_DB_FILENAME).is_file();
            let is_temp = is_temp_storage_dir(&storage_dir);
            let stale = entry.mode == "scan"
                || entry.pid == 0
                || now_ms.saturating_sub(updated_at_ms) > STALE_HEARTBEAT_MS;
            let next = ProjectInfo {
                project_guard: entry.project_guard,
                label: entry.label,
                storage_dir,
                workspace_default: entry.workspace_default,
                workspace_recommended: entry.workspace_recommended,
                updated_at_ms,
                stale,
                store_present,
                is_temp,
            };
            out.entry(next.project_guard.clone())
                .and_modify(|existing| {
                    if next.updated_at_ms > existing.updated_at_ms {
                        *existing = next.clone();
                    }
                })
                .or_insert(next);
        }
    }

    // When the registry dir is explicitly overridden (tests/dev), keep behavior deterministic
    // and do not merge in a user-wide catalog.
    if !registry_override_active() {
        for dir in catalog_dirs_for_read() {
            let Ok(read_dir) = std::fs::read_dir(&dir) else {
                continue;
            };
            for item in read_dir.flatten() {
                let path = item.path();
                if path.extension().and_then(|v| v.to_str()) != Some("json") {
                    continue;
                }
                let Some(entry) = read_entry(&path) else {
                    continue;
                };
                let updated_at_ms = entry.updated_at_ms;
                let storage_dir = PathBuf::from(entry.storage_dir);
                let store_present = storage_dir.join(STORE_DB_FILENAME).is_file();
                let is_temp = is_temp_storage_dir(&storage_dir);
                let stale = entry.mode == "scan"
                    || entry.pid == 0
                    || now_ms.saturating_sub(updated_at_ms) > STALE_HEARTBEAT_MS;
                let next = ProjectInfo {
                    project_guard: entry.project_guard,
                    label: entry.label,
                    storage_dir,
                    workspace_default: entry.workspace_default,
                    workspace_recommended: entry.workspace_recommended,
                    updated_at_ms,
                    stale,
                    store_present,
                    is_temp,
                };
                out.entry(next.project_guard.clone())
                    .and_modify(|existing| {
                        if next.updated_at_ms > existing.updated_at_ms {
                            *existing = next.clone();
                        }
                    })
                    .or_insert(next);
            }
        }
    }

    let mut out = out.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| match (a.stale, b.stale) {
        (false, true) => std::cmp::Ordering::Less,
        (true, false) => std::cmp::Ordering::Greater,
        _ => b
            .updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())),
    });
    out
}

pub(crate) fn lookup_project(project_guard: &str) -> Option<ProjectInfo> {
    let now_ms = now_ms_i64();
    let mut best: Option<ProjectInfo> = None;
    let mut dirs = registry_dirs_for_read();
    if !registry_override_active() {
        dirs.extend(catalog_dirs_for_read());
    }
    for dir in dirs {
        let path = entry_path(&dir, project_guard);
        let Some(entry) = read_entry(&path) else {
            continue;
        };
        let storage_dir = PathBuf::from(entry.storage_dir);
        let store_present = storage_dir.join(STORE_DB_FILENAME).is_file();
        let is_temp = is_temp_storage_dir(&storage_dir);
        let stale = entry.mode == "scan"
            || entry.pid == 0
            || now_ms.saturating_sub(entry.updated_at_ms) > STALE_HEARTBEAT_MS;
        let info = ProjectInfo {
            project_guard: entry.project_guard,
            label: entry.label,
            storage_dir,
            workspace_default: entry.workspace_default,
            workspace_recommended: entry.workspace_recommended,
            updated_at_ms: entry.updated_at_ms,
            stale,
            store_present,
            is_temp,
        };
        match best.as_ref() {
            Some(current) if current.updated_at_ms >= info.updated_at_ms => {}
            _ => best = Some(info),
        }
    }
    best
}

pub(crate) fn sync_catalog_from_presence() {
    if registry_override_active() {
        return;
    }
    let Some(catalog_dir) = catalog_dir_for_write() else {
        return;
    };

    for dir in registry_dirs_for_read() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let Some(entry) = read_entry(&path) else {
                continue;
            };

            let catalog_path = entry_path(&catalog_dir, &entry.project_guard);
            let should_write = match read_entry(&catalog_path) {
                Some(existing) => entry.updated_at_ms > existing.updated_at_ms,
                None => true,
            };
            if should_write {
                let _ = write_atomic_json(&catalog_path, &entry);
            }
        }
    }
}

pub(crate) fn sync_catalog_from_scan(primary_storage_dir: &Path) {
    if registry_override_active() {
        return;
    }
    let Some(catalog_dir) = catalog_dir_for_write() else {
        return;
    };

    let roots = scan_roots(primary_storage_dir);
    if roots.is_empty() {
        return;
    }

    let mut stores = Vec::<PathBuf>::new();
    for root in roots {
        scan_for_stores(&root, 0, &mut stores);
        if stores.len() >= MAX_SCAN_STORES {
            break;
        }
    }
    stores.sort();
    stores.dedup();

    let mut legacy_scan_entries_by_storage_dir: std::collections::HashMap<
        String,
        Vec<(PathBuf, RegistryEntry)>,
    > = std::collections::HashMap::new();
    if let Ok(read_dir) = std::fs::read_dir(&catalog_dir) {
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let Some(entry) = read_entry(&path) else {
                continue;
            };
            if entry.mode != "scan" {
                continue;
            }
            let canon = std::fs::canonicalize(&entry.storage_dir)
                .unwrap_or_else(|_| PathBuf::from(entry.storage_dir.clone()));
            legacy_scan_entries_by_storage_dir
                .entry(canon.to_string_lossy().to_string())
                .or_default()
                .push((path, entry));
        }
    }

    for storage_dir in stores {
        let storage_dir = std::fs::canonicalize(&storage_dir).unwrap_or(storage_dir);
        let storage_dir_key = storage_dir.to_string_lossy().to_string();
        let repo_root = repo_root_from_storage_dir(&storage_dir);

        let label = label_from_repo_root(&repo_root);
        let workspace = workspace_from_label(&label);
        let project_guard = project_guard_from_repo_root(&repo_root);
        let updated_at_ms = store_mtime_ms(&storage_dir).unwrap_or_else(now_ms_i64);

        if let Some(entries) = legacy_scan_entries_by_storage_dir.get(&storage_dir_key) {
            for (path, entry) in entries {
                if entry.project_guard != project_guard {
                    let _ = std::fs::remove_file(path);
                }
            }
        }

        let entry = RegistryEntry {
            project_guard: project_guard.clone(),
            label,
            storage_dir: storage_dir.to_string_lossy().to_string(),
            workspace_default: Some(workspace.clone()),
            workspace_recommended: Some(workspace),
            updated_at_ms,
            pid: 0,
            mode: "scan".to_string(),
        };

        let path = entry_path(&catalog_dir, &project_guard);
        let should_write = match read_entry(&path) {
            Some(existing) => {
                entry.updated_at_ms > existing.updated_at_ms
                    || existing.label != entry.label
                    || existing.storage_dir != entry.storage_dir
                    || existing.workspace_default != entry.workspace_default
                    || existing.workspace_recommended != entry.workspace_recommended
                    || existing.mode != entry.mode
                    || existing.project_guard != entry.project_guard
            }
            None => true,
        };
        if should_write {
            let _ = write_atomic_json(&path, &entry);
        }
    }
}

fn registry_dir_for_write() -> Option<PathBuf> {
    let candidates = registry_dirs_for_write_candidates();
    for dir in candidates {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if ensure_private_dir(&dir) {
            return Some(dir);
        }
    }
    None
}

fn registry_dirs_for_read() -> Vec<PathBuf> {
    if let Some(dir) = registry_dir_override() {
        return vec![dir];
    }
    let mut dirs = Vec::new();
    if let Some(dir) = registry_dir_from_xdg_env() {
        dirs.push(dir);
    }
    #[cfg(unix)]
    {
        if let Some(dir) = registry_dir_from_run_user() {
            dirs.push(dir);
        }
    }
    dirs.push(std::env::temp_dir().join(DEFAULT_DIR_NAME));
    dedup_dirs(dirs)
}

fn registry_dirs_for_write_candidates() -> Vec<PathBuf> {
    if let Some(dir) = registry_dir_override() {
        return vec![dir];
    }
    let mut dirs = Vec::new();
    if let Some(dir) = registry_dir_from_xdg_env() {
        dirs.push(dir);
    }
    #[cfg(unix)]
    {
        if let Some(dir) = registry_dir_from_run_user() {
            dirs.push(dir);
        }
    }
    dirs.push(std::env::temp_dir().join(DEFAULT_DIR_NAME));
    dedup_dirs(dirs)
}

fn catalog_dir_for_write() -> Option<PathBuf> {
    if registry_override_active() {
        return None;
    }
    catalog_dirs_for_write_candidates()
        .into_iter()
        .find(|dir| ensure_private_dir(dir))
}

fn catalog_dirs_for_read() -> Vec<PathBuf> {
    if registry_override_active() {
        return Vec::new();
    }
    if let Ok(raw) = std::env::var(CATALOG_DIR_ENV)
        && !raw.trim().is_empty()
    {
        return vec![PathBuf::from(raw)];
    }
    default_catalog_dir_candidates()
}

fn catalog_dirs_for_write_candidates() -> Vec<PathBuf> {
    if let Ok(raw) = std::env::var(CATALOG_DIR_ENV)
        && !raw.trim().is_empty()
    {
        return vec![PathBuf::from(raw)];
    }
    default_catalog_dir_candidates()
}

fn default_catalog_dir_candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(raw) = std::env::var("XDG_STATE_HOME")
        && !raw.trim().is_empty()
    {
        dirs.push(
            PathBuf::from(raw.trim())
                .join(DEFAULT_DIR_NAME)
                .join(DEFAULT_CATALOG_SUBDIR),
        );
    }
    if let Ok(raw) = std::env::var("HOME")
        && !raw.trim().is_empty()
    {
        dirs.push(
            PathBuf::from(raw.trim())
                .join(".local/state")
                .join(DEFAULT_DIR_NAME)
                .join(DEFAULT_CATALOG_SUBDIR),
        );
    }
    dirs.push(
        std::env::temp_dir()
            .join(DEFAULT_DIR_NAME)
            .join(DEFAULT_CATALOG_SUBDIR),
    );
    dedup_dirs(dirs)
}

fn registry_override_active() -> bool {
    std::env::var(REGISTRY_DIR_ENV)
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn registry_dir_override() -> Option<PathBuf> {
    std::env::var(REGISTRY_DIR_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn registry_dir_from_xdg_env() -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| PathBuf::from(v).join(DEFAULT_DIR_NAME))
}

#[cfg(unix)]
fn registry_dir_from_run_user() -> Option<PathBuf> {
    let uid = uid_from_proc_status()?;
    let base = PathBuf::from("/run/user").join(uid.to_string());
    if !base.is_dir() {
        return None;
    }
    Some(base.join(DEFAULT_DIR_NAME))
}

#[cfg(unix)]
fn uid_from_proc_status() -> Option<u32> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("Uid:") else {
            continue;
        };
        let uid_str = rest.split_whitespace().next()?;
        if let Ok(uid) = uid_str.parse::<u32>() {
            return Some(uid);
        }
    }
    None
}

fn dedup_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut out = Vec::new();
    for dir in dirs {
        let key = dir.to_string_lossy().to_string();
        if seen.insert(key) {
            out.push(dir);
        }
    }
    out
}

fn ensure_private_dir(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    true
}

fn entry_path(dir: &Path, project_guard: &str) -> PathBuf {
    let file = sanitize_guard(project_guard);
    dir.join(format!("{file}.json"))
}

fn sanitize_guard(value: &str) -> String {
    let mut out = String::with_capacity(value.len().max(8));
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "project".to_string()
    } else {
        out
    }
}

fn read_entry(path: &Path) -> Option<RegistryEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<RegistryEntry>(&text).ok()
}

fn write_atomic_json(path: &Path, entry: &RegistryEntry) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn scan_roots(primary_storage_dir: &Path) -> Vec<PathBuf> {
    if let Some(custom) = parse_scan_roots_env() {
        return custom;
    }

    let canonical = std::fs::canonicalize(primary_storage_dir)
        .unwrap_or_else(|_| primary_storage_dir.to_path_buf());
    let repo_root_storage = repo_root_from_storage_dir(&canonical);
    let repo_root_cwd = repo_root_from_cwd();
    let mut out = Vec::new();

    if let Some(projects_root) = ancestor_named(&repo_root_storage, "projects") {
        out.push(projects_root);
    } else if let Some(projects_root) = ancestor_named(&repo_root_cwd, "projects") {
        out.push(projects_root);
    } else {
        // Fall back to scanning a parent directory, but avoid common junk roots like /tmp when we
        // can. This commonly happens when the MCP server is launched with a non-repo-local
        // storage dir but the current working directory is inside a git repo.
        let temp = std::env::temp_dir();
        let parent_storage = repo_root_storage.parent().map(|p| p.to_path_buf());
        let parent_cwd = repo_root_cwd.parent().map(|p| p.to_path_buf());
        if let Some(parent) = parent_storage.as_ref().filter(|p| !p.starts_with(&temp)) {
            out.push(parent.clone());
        } else if let Some(parent) = parent_cwd.as_ref().filter(|p| !p.starts_with(&temp)) {
            out.push(parent.clone());
        } else if let Some(parent) = parent_cwd.or(parent_storage) {
            out.push(parent);
        }
    }

    // Conservative fallback: don't scan $HOME by default unless we can anchor to a known
    // projects-like root. The env override exists for power users.
    dedup_dirs(out)
}

fn parse_scan_roots_env() -> Option<Vec<PathBuf>> {
    let Ok(raw) = std::env::var(SCAN_ROOTS_ENV) else {
        return None;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in trimmed.split(':') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        out.push(PathBuf::from(part));
    }
    Some(dedup_dirs(out))
}

fn ancestor_named(path: &Path, name: &str) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        let Some(file_name) = ancestor.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if file_name.eq_ignore_ascii_case(name) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn scan_for_stores(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > DEFAULT_SCAN_MAX_DEPTH {
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        if out.len() >= MAX_SCAN_STORES {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();

        if name.as_ref() == STORE_DIRNAME {
            if path.join(STORE_DB_FILENAME).is_file() {
                out.push(path);
            }
            continue;
        }

        if name.starts_with('.') {
            continue;
        }
        if matches!(name.as_ref(), "target" | "node_modules" | "dist" | "build") {
            continue;
        }

        scan_for_stores(&path, depth + 1, out);
    }
}

fn repo_root_from_storage_dir(storage_dir: &Path) -> PathBuf {
    let Some(dir_name) = storage_dir.file_name().and_then(|v| v.to_str()) else {
        return storage_dir.to_path_buf();
    };
    if dir_name == STORE_DIRNAME
        && let Some(parent) = storage_dir.parent()
    {
        // Prefer the parent directory even when the project isn't a git repo.
        // This matches how default workspace/project_guard are derived in auto-mode.
        return parent.to_path_buf();
    }
    storage_dir.to_path_buf()
}

fn repo_root_from_cwd() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            break;
        }
    }
    cwd
}

fn label_from_repo_root(repo_root: &Path) -> String {
    repo_root
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("project")
        .trim()
        .to_string()
}

fn workspace_from_label(label: &str) -> String {
    let raw = label.trim();
    let mut out = String::with_capacity(raw.len().max(8));
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

fn project_guard_from_repo_root(repo_root: &Path) -> String {
    let canonical = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let canonical_str = canonical.to_string_lossy();
    let bytes = canonical_str.as_bytes();
    // FNV-1a 64-bit, aligned with `support/runtime.rs`.
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("repo:{hash:016x}")
}

fn store_mtime_ms(storage_dir: &Path) -> Option<i64> {
    let db_path = storage_dir.join(STORE_DB_FILENAME);
    let meta = db_path.metadata().ok()?;
    let modified = meta.modified().ok()?;
    let unix = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    let ms = unix.as_millis();
    i64::try_from(ms).ok()
}

fn is_temp_storage_dir(storage_dir: &Path) -> bool {
    let temp = std::env::temp_dir();
    let canon = std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
    canon.starts_with(&temp)
}
