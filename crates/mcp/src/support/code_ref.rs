#![forbid(unsafe_code)]

use super::ai::{ai_error, warning};
use super::repo_paths::{normalize_repo_rel, repo_rel_from_path_input};
use bm_core::ids::WorkspaceId;
use serde_json::Value;
use sha2::Digest as _;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct CodeRef {
    pub repo_rel: String,
    pub start_line: u32,
    pub end_line: u32,
    pub sha256: Option<String>,
}

fn parse_u32(raw: &str, field: &str) -> Result<u32, Value> {
    raw.trim()
        .parse::<u32>()
        .map_err(|_| ai_error("INVALID_INPUT", &format!("{field}: expected integer")))
}

fn parse_sha256_hex(raw: &str) -> Result<String, Value> {
    let s = raw.trim();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ai_error(
            "INVALID_INPUT",
            "sha256: expected 64 hex characters",
        ));
    }
    Ok(s.to_ascii_lowercase())
}

pub(crate) fn parse_code_ref(raw: &str) -> Result<Option<CodeRef>, Value> {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("code:") else {
        return Ok(None);
    };

    let (path_raw, rest) = rest
        .split_once("#L")
        .ok_or_else(|| ai_error("INVALID_INPUT", "CODE_REF: expected '#L'"))?;
    let (start_raw, rest) = rest
        .split_once("-L")
        .ok_or_else(|| ai_error("INVALID_INPUT", "CODE_REF: expected '-L'"))?;
    let (end_raw, sha_raw) = match rest.split_once("@sha256:") {
        Some((end, sha)) => (end, Some(sha)),
        None => (rest, None),
    };

    let start_line = parse_u32(start_raw, "start_line")?;
    let end_line = parse_u32(end_raw, "end_line")?;
    if start_line == 0 || end_line == 0 || end_line < start_line {
        return Err(ai_error(
            "INVALID_INPUT",
            "CODE_REF: invalid line range (expected 1-based start<=end)",
        ));
    }
    let sha256 = match sha_raw {
        Some(v) => Some(parse_sha256_hex(v)?),
        None => None,
    };

    // Keep repo_rel normalization for the validator; here we only preserve the raw path.
    let path_raw = path_raw.trim();
    if path_raw.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            "CODE_REF: path must not be empty",
        ));
    }

    Ok(Some(CodeRef {
        repo_rel: path_raw.to_string(),
        start_line,
        end_line,
        sha256,
    }))
}

pub(crate) fn parse_code_ref_required(raw: &str, field: &str) -> Result<CodeRef, Value> {
    let Some(parsed) = parse_code_ref(raw)? else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected CODE_REF format (code:...#Lx-Ly[@sha256:...])"),
        ));
    };
    Ok(parsed)
}

fn sha256_file_hex(path: &Path) -> Result<String, std::io::Error> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = sha2::Sha256::new();

    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(&mut out, "{:02x}", b);
    }
    Ok(out)
}

fn count_lines(path: &Path, max_bytes: u64) -> Result<Option<u32>, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Ok(None);
    }
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; 16 * 1024];
    let mut lines = 0u32;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        lines += buf[..n].iter().filter(|b| **b == b'\n').count() as u32;
    }
    // Count final line even if file does not end with '\n' (best-effort).
    Ok(Some(lines.saturating_add(1)))
}

pub(crate) struct CodeRefValidation {
    pub normalized: String,
    pub warnings: Vec<Value>,
}

pub(crate) fn validate_code_ref(
    store: &bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    code_ref: &CodeRef,
) -> Result<CodeRefValidation, Value> {
    let root = store
        .workspace_path_primary_get(workspace)
        .map_err(|e| ai_error("STORE_ERROR", &format!("{e:?}")))?;

    let repo_root = root.map(PathBuf::from);
    let repo_rel = match &repo_root {
        Some(root) => repo_rel_from_path_input(&code_ref.repo_rel, Some(root))?,
        None => normalize_repo_rel(&code_ref.repo_rel)?,
    };

    let sha_fallback = "0".repeat(64);
    let sha_from_input = code_ref.sha256.clone().unwrap_or(sha_fallback);
    let mut normalized = format!(
        "code:{repo_rel}#L{}-L{}@sha256:{}",
        code_ref.start_line, code_ref.end_line, sha_from_input
    );

    let mut warnings = Vec::<Value>::new();

    let Some(repo_root) = repo_root else {
        warnings.push(warning(
            "CODE_REF_UNRESOLVABLE",
            "workspace has no bound path; cannot validate CODE_REF sha256",
            "Bind the workspace to a repo path first (e.g. call status with workspace=\"/path/to/repo\").",
        ));
        return Ok(CodeRefValidation {
            normalized,
            warnings,
        });
    };

    let abs = repo_root.join(&repo_rel);
    if !abs.exists() {
        warnings.push(warning(
            "CODE_REF_MISSING",
            &format!(
                "CODE_REF file does not exist under workspace root: repo_rel={repo_rel} root={}",
                repo_root.to_string_lossy()
            ),
            "Refresh the CODE_REF or ensure the path is under the workspace bound root.",
        ));
        return Ok(CodeRefValidation {
            normalized,
            warnings,
        });
    }

    // Validate sha256 drift (required). Treat mismatch as stale warning (message is still accepted).
    let current = sha256_file_hex(&abs).map_err(|_| {
        ai_error(
            "INVALID_INPUT",
            "CODE_REF: failed to read file for sha256 validation",
        )
    })?;
    if let Some(sha) = code_ref.sha256.as_deref()
        && current != sha
    {
        warnings.push(warning(
            "CODE_REF_STALE",
            "CODE_REF sha256 does not match current file content",
            "Refresh the CODE_REF (recompute sha256 at the current version) or accept drift explicitly.",
        ));
    }
    // Always normalize the sha256 segment to the current file content hash. This keeps stored
    // CODE_REF values deterministic and avoids downstream drift cascade when an agent cannot (or
    // does not) compute sha256 correctly.
    //
    // Also validate+normalize line ranges best-effort (bounded by file size). When a range exceeds
    // file length (common for LLM-generated refs), clamp deterministically so downstream tools can
    // still open the reference without failing hard.
    const MAX_LINECOUNT_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
    let mut start_line = code_ref.start_line;
    let mut end_line = code_ref.end_line;
    if let Ok(Some(line_count)) = count_lines(&abs, MAX_LINECOUNT_BYTES) {
        let orig_start = start_line;
        let orig_end = end_line;

        if start_line > line_count {
            start_line = line_count.max(1);
        }
        if end_line > line_count {
            end_line = line_count.max(1);
        }
        if end_line < start_line {
            end_line = start_line;
        }

        if (orig_start, orig_end) != (start_line, end_line) {
            warnings.push(warning(
                "CODE_REF_RANGE_NORMALIZED",
                &format!(
                    "CODE_REF line range was clamped to current file length ({repo_rel}: L{orig_start}-L{orig_end} → L{start_line}-L{end_line})"
                ),
                "Regenerate CODE_REF line ranges from the current repo state if you need precise anchors.",
            ));
        }
    }

    normalized = format!("code:{repo_rel}#L{start_line}-L{end_line}@sha256:{current}");

    Ok(CodeRefValidation {
        normalized,
        warnings,
    })
}
