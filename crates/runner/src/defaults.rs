#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_STORE_DIRNAME: &str = ".agents/mcp/.branchmind";

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let git = current.join(".git");
        if git.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn normalize_workspace_id(raw: &str) -> String {
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

pub(crate) fn default_storage_dir_from_start(start: &Path) -> PathBuf {
    // Align with `bm_mcp` defaults: prefer repo-root store, not CWD.
    // This prevents "runner is running but sees no jobs" when launched from a subdirectory.
    find_repo_root(start)
        .unwrap_or_else(|| start.to_path_buf())
        .join(DEFAULT_STORE_DIRNAME)
}

pub(crate) fn default_workspace_from_start(start: &Path) -> String {
    // Align with `bm_mcp`: workspace derives from repo root name (sanitized),
    // not from whichever subdirectory the runner was started in.
    let root = find_repo_root(start).unwrap_or_else(|| start.to_path_buf());
    let raw = root
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("workspace");
    normalize_workspace_id(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let dir = base.join(format!("{prefix}_{pid}_{nonce}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn defaults_prefer_repo_root_over_subdir() {
        let root = temp_dir("bm_runner_defaults_repo_root");
        std::fs::create_dir_all(root.join(".git")).expect("create fake .git");
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("create nested dir");

        let storage = default_storage_dir_from_start(&nested);
        assert_eq!(storage, root.join(DEFAULT_STORE_DIRNAME));

        let ws = default_workspace_from_start(&nested);
        let expected = normalize_workspace_id(
            root.file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("workspace"),
        );
        assert_eq!(ws, expected);
    }
}
