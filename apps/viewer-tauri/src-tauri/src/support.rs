#![forbid(unsafe_code)]

use bm_storage::{SqliteStore, StoreError};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn default_scan_roots() -> Vec<PathBuf> {
    if let Ok(raw) = std::env::var("BRANCHMIND_VIEWER_SCAN_ROOTS") {
        let mut out = Vec::new();
        for part in raw.split(|ch| ch == ';' || ch == ',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.push(PathBuf::from(trimmed));
        }
        if !out.is_empty() {
            return out;
        }
    }

    let mut out = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        out.push(PathBuf::from(&home).join("Документы").join("projects"));
        out.push(PathBuf::from(&home).join("Documents").join("projects"));
        out.push(PathBuf::from(&home).join("projects"));
    }

    // Common Linux mount layout for secondary disks / shared partitions:
    // `/media/<user>/documents/projects` (or localized variants).
    if let Ok(user) = std::env::var("USER") {
        if !user.trim().is_empty() {
            out.push(PathBuf::from("/media").join(&user).join("documents").join("projects"));
            out.push(PathBuf::from("/media").join(&user).join("Документы").join("projects"));
            out.push(PathBuf::from("/media").join(&user).join("Documents").join("projects"));
        }
    }
    out
}

fn should_skip_dir_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".cache"
            | ".cargo"
            | ".rustup"
            | ".venv"
            | "__pycache__"
    )
}

pub fn scan_storage_dirs(
    roots: Vec<PathBuf>,
    max_depth: usize,
    limit: usize,
    timeout_ms: u64,
) -> Result<Vec<PathBuf>, String> {
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms.max(50));

    let mut found = BTreeSet::<PathBuf>::new();
    let mut stack = Vec::<(PathBuf, usize)>::new();

    for root in roots {
        if stack.len() >= 10_000 {
            break;
        }
        stack.push((root, 0));
    }

    while let Some((dir, depth)) = stack.pop() {
        if Instant::now() > deadline {
            break;
        }
        if found.len() >= limit.max(1) {
            break;
        }
        if depth > max_depth {
            continue;
        }

        let dir_name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if should_skip_dir_name(dir_name) {
            continue;
        }

        // Storage dir patterns (read-only viewer):
        //
        // 1) repo-local default: <repo>/.agents/mcp/.branchmind/branchmind_rust.db
        // 2) daemon default:     <repo>/.branchmind_rust/branchmind_rust.db
        // 3) legacy/dev:         <dir>/branchmind_rust.db (direct)
        // 4) legacy/dev:         <repo>/.branchmind/branchmind_rust.db
        let direct_db = dir.join("branchmind_rust.db");
        if direct_db.is_file() {
            found.insert(dir.clone());
            continue;
        }

        let branchmind_rust_db = dir.join(".branchmind_rust").join("branchmind_rust.db");
        if branchmind_rust_db.is_file() {
            found.insert(branchmind_rust_db.parent().expect("db parent").to_path_buf());
            continue;
        }

        let legacy_branchmind_db = dir.join(".branchmind").join("branchmind_rust.db");
        if legacy_branchmind_db.is_file() {
            found.insert(legacy_branchmind_db.parent().expect("db parent").to_path_buf());
            continue;
        }

        // Fast-path: a repo root commonly contains `.agents/mcp/.branchmind/branchmind_rust.db`.
        let candidate_db = dir
            .join(".agents")
            .join("mcp")
            .join(".branchmind")
            .join("branchmind_rust.db");
        if candidate_db.is_file() {
            found.insert(
                candidate_db
                    .parent()
                    .expect("db parent")
                    .to_path_buf(),
            );
            continue;
        }

        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            stack.push((path, depth + 1));
        }
    }

    Ok(found.into_iter().collect())
}

pub fn canonicalize_best_effort(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn validate_storage_dir(storage_dir: &str) -> Result<PathBuf, String> {
    let trimmed = storage_dir.trim();
    if trimmed.is_empty() {
        return Err("storage_dir must not be empty".to_string());
    }
    let path = PathBuf::from(trimmed);
    let canon = canonicalize_best_effort(&path);
    let db = canon.join("branchmind_rust.db");
    if !db.is_file() {
        return Err(format!(
            "storage_dir does not contain branchmind_rust.db: {}",
            canon.to_string_lossy()
        ));
    }
    Ok(canon)
}

pub fn open_store_read_only(storage_dir: &Path) -> Result<SqliteStore, String> {
    SqliteStore::open_read_only(storage_dir).map_err(store_err_to_string)
}

pub fn store_err_to_string(err: StoreError) -> String {
    match err {
        StoreError::UnknownId => "UNKNOWN_ID".to_string(),
        StoreError::UnknownBranch => "UNKNOWN_BRANCH".to_string(),
        StoreError::UnknownConflict => "UNKNOWN_CONFLICT".to_string(),
        StoreError::StepNotFound => "STEP_NOT_FOUND".to_string(),
        StoreError::InvalidInput(msg) => format!("INVALID_INPUT: {msg}"),
        other => format!("{other:?}"),
    }
}

pub fn guess_repo_root(storage_dir: &Path) -> Option<PathBuf> {
    let mut cur = storage_dir;
    for _ in 0..10 {
        let git = cur.join(".git");
        if git.is_dir() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
    None
}
