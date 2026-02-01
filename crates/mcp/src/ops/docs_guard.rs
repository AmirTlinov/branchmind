#![forbid(unsafe_code)]

use crate::ops::DocRef;
use std::fs;
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
    None
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
    ];
    candidates.iter().any(|needle| content.contains(needle))
}

pub(crate) fn doc_ref_exists(doc_ref: &DocRef) -> bool {
    doc_ref_path_exists(doc_ref) && doc_ref_anchor_exists(doc_ref)
}
