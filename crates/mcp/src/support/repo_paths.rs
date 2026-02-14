#![forbid(unsafe_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) fn normalize_repo_rel(raw: &str) -> Result<String, Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(crate::ai_error("INVALID_INPUT", "path must not be empty"));
    }

    // Normalize separators first (Windows-friendly).
    let normalized = raw.replace('\\', "/");
    let mut out = Vec::<String>::new();
    for part in normalized.split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(crate::ai_error(
                "INVALID_INPUT",
                "path must not contain '..' segments",
            ));
        }
        out.push(part.to_string());
    }
    if out.is_empty() {
        return Ok(".".to_string());
    }
    Ok(out.join("/"))
}

pub(crate) fn repo_rel_from_path_input(
    raw: &str,
    repo_root: Option<&Path>,
) -> Result<String, Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(crate::ai_error("INVALID_INPUT", "path must not be empty"));
    }

    let expanded = if raw == "~" || raw.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            if raw == "~" {
                home
            } else {
                home.join(raw.trim_start_matches("~/"))
            }
        } else {
            PathBuf::from(raw)
        }
    } else {
        PathBuf::from(raw)
    };

    // Repo-relative paths are treated as repo-relative (not CWD-relative).
    if !expanded.is_absolute() {
        return normalize_repo_rel(raw);
    }

    let mut absolute = expanded;
    if let Ok(canon) = std::fs::canonicalize(&absolute) {
        absolute = canon;
    }

    if absolute.is_absolute() {
        let Some(repo_root) = repo_root else {
            return Err(crate::ai_error_with(
                "INVALID_INPUT",
                "workspace has no bound path; cannot resolve absolute path",
                Some(
                    "Bind the workspace to a repo path first (e.g. call status with workspace=\"/path/to/repo\").",
                ),
                vec![],
            ));
        };
        let mut root = repo_root.to_path_buf();
        if let Ok(canon) = std::fs::canonicalize(&root) {
            root = canon;
        }

        let rel = absolute.strip_prefix(&root).map_err(|_| {
            crate::ai_error_with(
                "INVALID_INPUT",
                "path is not under the workspace bound root",
                Some(&format!(
                    "path={} root={}",
                    absolute.to_string_lossy(),
                    root.to_string_lossy()
                )),
                vec![],
            )
        })?;

        let mut parts = Vec::<String>::new();
        for comp in rel.components() {
            match comp {
                std::path::Component::Normal(v) => parts.push(v.to_string_lossy().to_string()),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    return Err(crate::ai_error(
                        "INVALID_INPUT",
                        "path must not escape the repo root",
                    ));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            return Ok(".".to_string());
        }
        return normalize_repo_rel(&parts.join("/"));
    }

    normalize_repo_rel(raw)
}
