#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let Some(git_dir) = find_git_dir(&manifest_dir) else {
        return;
    };

    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    let Ok(head_raw) = fs::read_to_string(&head_path) else {
        return;
    };

    let head = head_raw.trim();
    if head.is_empty() {
        return;
    }

    let sha = if let Some(ref_path) = head.strip_prefix("ref:").map(|s| s.trim()) {
        resolve_ref(&git_dir, ref_path)
    } else {
        Some(head.to_string())
    };

    let Some(sha) = sha else {
        return;
    };

    let sha = sha.trim();
    if sha.is_empty() {
        return;
    }

    let short = sha.chars().take(12).collect::<String>();
    println!("cargo:rustc-env=BM_GIT_SHA={short}");
}

fn find_git_dir(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let dot_git = current.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            let Ok(text) = fs::read_to_string(&dot_git) else {
                return None;
            };
            let line = text.lines().next().unwrap_or("").trim();
            if let Some(path) = line.strip_prefix("gitdir:").map(|s| s.trim()) {
                let resolved = current.join(path);
                return Some(resolved);
            }
            return None;
        }

        current = current.parent()?;
    }
}

fn resolve_ref(git_dir: &Path, ref_path: &str) -> Option<String> {
    // If it is a loose ref, read it directly.
    let full_ref = git_dir.join(ref_path);
    if full_ref.exists() {
        println!("cargo:rerun-if-changed={}", full_ref.display());
        if let Ok(text) = fs::read_to_string(&full_ref) {
            let sha = text.trim().to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
    }

    // Otherwise, try packed-refs (common in worktrees / after gc).
    let packed = git_dir.join("packed-refs");
    if packed.exists() {
        println!("cargo:rerun-if-changed={}", packed.display());
        if let Ok(text) = fs::read_to_string(&packed) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                    continue;
                }
                let Some((sha, name)) = line.split_once(' ') else {
                    continue;
                };
                if name == ref_path {
                    let sha = sha.trim().to_string();
                    if !sha.is_empty() {
                        return Some(sha);
                    }
                }
            }
        }
    }

    None
}
