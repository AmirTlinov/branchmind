#![forbid(unsafe_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub(super) struct SessionMeta {
    pub(super) id: Option<String>,
    pub(super) ts: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) path_hints: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ExtractedMessage {
    pub(super) role: String,
    pub(super) ts: Option<String>,
    pub(super) text: String,
}

#[derive(Clone, Debug)]
pub(super) struct ProjectHint {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) confidence: u8,
}

pub(super) fn default_codex_sessions_dir() -> String {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(home)
            .join("sessions")
            .to_string_lossy()
            .to_string();
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".codex")
            .join("sessions")
            .to_string_lossy()
            .to_string();
    }
    PathBuf::from(".codex")
        .join("sessions")
        .to_string_lossy()
        .to_string()
}

pub(super) fn project_cwd_prefix_from_storage_dir(storage_dir: &Path) -> String {
    let base = storage_dir.parent().unwrap_or(storage_dir);
    let canon = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let (root, confidence) = find_repo_root(&canon);
    if confidence >= 60 {
        root.to_string_lossy().to_string()
    } else {
        canon.to_string_lossy().to_string()
    }
}

pub(super) fn canonicalize_existing_dir(root_dir: &Path) -> std::io::Result<PathBuf> {
    let canon = std::fs::canonicalize(root_dir)?;
    let meta = std::fs::metadata(&canon)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "root_dir must be a directory",
        ));
    }
    Ok(canon)
}

pub(super) fn canonicalize_existing_file(path: &Path) -> std::io::Result<PathBuf> {
    let canon = std::fs::canonicalize(path)?;
    let meta = std::fs::metadata(&canon)?;
    if !meta.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "ref.path must point to a file",
        ));
    }
    Ok(canon)
}

pub(super) fn is_within_root(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

pub(super) fn list_jsonl_files_deterministic(root: &Path, max_files: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    visit_dir(root, &mut out, max_files, true);
    out
}

fn visit_dir(dir: &Path, out: &mut Vec<PathBuf>, max_files: usize, newest_first: bool) {
    if out.len() >= max_files {
        return;
    }
    let read = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    if newest_first {
        entries.reverse();
    }
    for entry in entries {
        if out.len() >= max_files {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            visit_dir(&path, out, max_files, newest_first);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        {
            out.push(path);
        }
    }
}

pub(super) fn parse_session_meta(value: &Value) -> Option<SessionMeta> {
    if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?.as_object()?;
    let id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let ts = payload
        .get("timestamp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut path_hints = Vec::new();
    if let Some(instructions) = payload.get("instructions").and_then(|v| v.as_str()) {
        path_hints = extract_path_hints_from_text(instructions);
    }
    Some(SessionMeta {
        id,
        ts,
        cwd,
        path_hints,
    })
}

pub(super) fn extract_message(value: &Value) -> Option<ExtractedMessage> {
    // Codex transcript format: response_item payload.type=message, payload.role, payload.content[].text
    let payload = value.get("payload")?.as_object()?;
    if payload.get("type").and_then(|v| v.as_str()) != Some("message") {
        return None;
    }
    let role = payload.get("role")?.as_str()?.to_string();
    let ts = value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content = payload.get("content")?.as_array()?;
    let mut parts = Vec::new();
    for item in content {
        let Some(obj) = item.as_object() else {
            continue;
        };
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(ExtractedMessage {
        role,
        ts,
        text: parts.join("\n"),
    })
}

pub(super) fn session_matches_cwd_prefix(meta: &SessionMeta, cwd_prefix: &str) -> bool {
    if cwd_prefix.is_empty() {
        return true;
    }
    if meta
        .cwd
        .as_deref()
        .is_some_and(|cwd| cwd.starts_with(cwd_prefix))
    {
        return true;
    }
    if meta
        .path_hints
        .iter()
        .any(|hint| hint.starts_with(cwd_prefix))
    {
        return true;
    }

    // Canonicalize-based fallback: lets a session cwd like `/home/...` match a prefix like
    // `/media/...` when those are the same directory via symlink/bind, and also handles `..`
    // segments. Deterministic and bounded (only a few candidates per session).
    let prefix_raw = cwd_prefix.trim();
    if prefix_raw.is_empty() {
        return true;
    }

    let prefix_path = PathBuf::from(prefix_raw);
    let prefix_canon = std::fs::canonicalize(&prefix_path).unwrap_or(prefix_path);

    let mut candidates: Vec<&str> = Vec::new();
    if let Some(cwd) = meta.cwd.as_deref() {
        candidates.push(cwd);
    }
    for hint in &meta.path_hints {
        candidates.push(hint);
    }

    for raw in candidates {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        let canon = std::fs::canonicalize(&path).unwrap_or(path);
        if canon.starts_with(&prefix_canon) {
            return true;
        }
    }

    false
}

pub(super) fn infer_project_hint(meta: &SessionMeta, fallback_id_seed: &str) -> ProjectHint {
    let mut candidates = Vec::new();
    if let Some(cwd) = meta.cwd.as_deref() {
        candidates.push(cwd.to_string());
    }
    candidates.extend(meta.path_hints.iter().cloned());

    let mut best_root: Option<PathBuf> = None;
    let mut best_confidence: u8 = 0;

    for raw in candidates {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        let canon = match std::fs::canonicalize(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Ok(meta_fs) = std::fs::metadata(&canon) else {
            continue;
        };
        if !meta_fs.is_dir() {
            continue;
        }

        let (root, confidence) = find_repo_root(&canon);
        if confidence == 0 {
            continue;
        }
        if confidence > best_confidence {
            best_confidence = confidence;
            best_root = Some(root);
        } else if confidence == best_confidence {
            let current = root.to_string_lossy().len();
            let best = best_root
                .as_ref()
                .map(|p| p.to_string_lossy().len())
                .unwrap_or(0);
            if current > best {
                best_root = Some(root);
            }
        }
    }

    let (seed, name, confidence) = match best_root {
        Some(root) => {
            let seed = root.to_string_lossy().to_string();
            let name = root
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("project")
                .to_string();
            (seed, name, best_confidence)
        }
        None => (fallback_id_seed.to_string(), "unknown".to_string(), 0),
    };

    let id = format!("{:016x}", fnv1a64(seed.as_bytes()));
    ProjectHint {
        id,
        name,
        confidence,
    }
}

pub(super) fn normalize_snippet_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        let is_space = ch.is_whitespace();
        if is_space {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
            continue;
        }
        last_space = false;
        out.push(ch);
    }
    out.trim().to_string()
}

pub(super) fn slice_around_match(
    text: &str,
    match_byte: usize,
    match_len_bytes: usize,
    context_chars: usize,
) -> String {
    if context_chars == 0 {
        return String::new();
    }

    let match_end_byte = (match_byte + match_len_bytes).min(text.len());

    let mut match_start_char: Option<usize> = None;
    let mut match_end_char: Option<usize> = None;
    for (char_index, (byte_idx, _)) in text.char_indices().enumerate() {
        if byte_idx == match_byte {
            match_start_char = Some(char_index);
        }
        if byte_idx == match_end_byte {
            match_end_char = Some(char_index);
            break;
        }
    }
    let total_chars = text.chars().count();
    let match_start_char = match_start_char.unwrap_or(0);
    let match_end_char = match_end_char.unwrap_or(total_chars);

    let half = context_chars / 2;
    let start_char = match_start_char.saturating_sub(half);
    let end_char = (match_end_char + half).min(total_chars);

    let mut start_byte = 0usize;
    let mut end_byte = text.len();
    if start_char > 0 || end_char < total_chars {
        for (ci, (byte_idx, _)) in text.char_indices().enumerate() {
            if ci == start_char {
                start_byte = byte_idx;
            }
            if ci == end_char {
                end_byte = byte_idx;
                break;
            }
        }
    }
    normalize_snippet_whitespace(&text[start_byte..end_byte])
}

pub(super) fn extract_path_hints_from_text(text: &str) -> Vec<String> {
    // Deterministic + bounded: scan only the first chunk.
    let scan = if text.len() > 64 * 1024 {
        &text[..64 * 1024]
    } else {
        text
    };
    let mut out = Vec::new();

    // Pattern 1: <cwd>...</cwd>
    let mut rest = scan;
    while let Some(start) = rest.find("<cwd>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find("</cwd>") else {
            break;
        };
        let value = after[..end].trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
        rest = &after[end + 6..];
    }

    // Pattern 2: "AGENTS.md instructions for <path>"
    let marker = "AGENTS.md instructions for ";
    for line in scan.lines() {
        let Some(pos) = line.find(marker) else {
            continue;
        };
        let value = line[pos + marker.len()..].trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
    }

    out.sort();
    out.dedup();
    out
}

fn find_repo_root(start: &Path) -> (PathBuf, u8) {
    // Walk up from start until we find a repo marker. This avoids executing `git`.
    let mut current = Some(start);
    let mut best: Option<(PathBuf, u8)> = None;
    while let Some(dir) = current {
        if has_dir_marker(dir, ".git") || has_file_marker(dir, ".git") {
            return (dir.to_path_buf(), 95);
        }
        if has_file_marker(dir, "Cargo.toml")
            || has_file_marker(dir, "package.json")
            || has_file_marker(dir, "pyproject.toml")
            || has_file_marker(dir, "go.mod")
        {
            best = Some((dir.to_path_buf(), 60));
        }
        current = dir.parent();
    }
    best.unwrap_or((start.to_path_buf(), 0))
}

fn has_dir_marker(dir: &Path, name: &str) -> bool {
    let marker = dir.join(name);
    std::fs::metadata(marker)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

fn has_file_marker(dir: &Path, name: &str) -> bool {
    let marker = dir.join(name);
    std::fs::metadata(marker)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
