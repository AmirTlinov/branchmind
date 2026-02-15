#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 2000;
const ALLOWLIST: &[&str] = &[];

#[test]
fn rust_source_files_stay_under_max_lines() {
    let root = workspace_root();
    let crate_root = root.join("crates");
    let allowlist = ALLOWLIST.iter().copied().collect::<HashSet<_>>();

    let mut offenders = Vec::new();
    for path in collect_rs_files(&crate_root) {
        if !path.components().any(|c| c.as_os_str() == "src") {
            continue;
        }
        let rel = relative_path(&root, &path);
        if allowlist.contains(rel.as_str()) {
            continue;
        }
        let contents =
            fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {rel}: {err}"));
        let lines = contents.lines().count();
        if lines > MAX_LINES {
            offenders.push((rel, lines));
        }
    }

    if !offenders.is_empty() {
        offenders.sort_by(|a, b| b.1.cmp(&a.1));
        let mut details = String::new();
        for (path, lines) in offenders {
            details.push_str(&format!("\n- {path}: {lines} lines"));
        }
        panic!(
            "structural gate: Rust source files must be <= {MAX_LINES} lines.{details}\n\
Add a split or update the allowlist intentionally."
        );
    }
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("failed to read dir {}: {err}", dir.display()));
        for entry in entries {
            let entry = entry
                .unwrap_or_else(|err| panic!("failed to read entry in {}: {err}", dir.display()));
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if should_skip_dir(name) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension() == Some(OsStr::new("rs")) {
                out.push(path);
            }
        }
    }
    out
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | "tmp"
            | ".agents"
            | ".branchmind"
            | ".context-finder"
            | ".last"
            | "node_modules"
    )
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
