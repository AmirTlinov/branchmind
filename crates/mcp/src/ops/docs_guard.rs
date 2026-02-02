#![forbid(unsafe_code)]

use crate::ops::DocRef;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn resolve_doc_path(path: &str) -> Option<PathBuf> {
    let raw = PathBuf::from(path);
    if raw.exists() {
        return Some(raw);
    }
    let mut current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let candidate = current.join(path);
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }

    // Daemon/shared mode can run with a CWD outside the repo root. When that happens,
    // resolving docs/contracts/* relative to CWD will fail and create noisy DOCS_DRIFT
    // warnings in portal flows (schema.get, help, etc.). We treat the repo-local storage
    // dir as an additional base path to keep doc_ref resolution stable across modes.
    //
    // This stays deterministic:
    // - the storage dir is deterministic for a given daemon invocation
    // - we never touch the network or external processes
    let storage_dir = crate::parse_storage_dir();
    let storage_dir =
        std::fs::canonicalize(&storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
    if let Some(repo_root) = repo_root_from_storage_dir(&storage_dir) {
        let candidate = repo_root.join(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
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

pub(crate) fn doc_ref_path_exists(doc_ref: &DocRef) -> bool {
    let path = doc_ref.path.trim();
    if path.is_empty() {
        return false;
    }
    resolve_doc_path(path)
        .and_then(|p| fs::metadata(p).ok())
        .is_some()
}

pub(crate) fn doc_ref_anchor_exists(doc_ref: &DocRef) -> bool {
    let path = doc_ref.path.trim();
    if path.is_empty() {
        return false;
    }
    let Some(path) = resolve_doc_path(path) else {
        return false;
    };
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let anchor = doc_ref.anchor.trim_start_matches('#');
    if anchor.is_empty() {
        return false;
    }
    let candidates = [
        format!("#{anchor}"),
        format!("# {anchor}"),
        format!("## {anchor}"),
        format!("### {anchor}"),
        format!("#### {anchor}"),
        // Pandoc-style heading identifiers: `## Title {#my-anchor}`
        format!("{{#{anchor}}}"),
        // HTML-style identifiers (e.g. rendered docs).
        format!("id=\"{anchor}\""),
        format!("id='{anchor}'"),
    ];
    candidates.iter().any(|needle| content.contains(needle))
}

pub(crate) fn doc_ref_exists(doc_ref: &DocRef) -> bool {
    doc_ref_path_exists(doc_ref) && doc_ref_anchor_exists(doc_ref)
}
