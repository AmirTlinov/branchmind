#![forbid(unsafe_code)]

use sha2::Digest;
use std::fmt::Write as _;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};

use crate::support::runtime::repo_root_from_storage_dir_strict;

pub(crate) const CODE_REF_PREFIX: &str = "code:";
const CODE_REF_HASH_TAG: &str = "@sha256:";

/// Hard safety rail: the `code:` ref itself must be reasonably small.
/// The output can still be trimmed further via `max_chars` budgets.
const CODE_REF_MAX_LINES: usize = 240;

/// Default budget for `open id=code:*` when the caller does not provide `max_chars`.
pub(crate) const CODE_REF_DEFAULT_MAX_CHARS: usize = 12_000;

/// Reserve some space for metadata around the snippet.
const CODE_REF_BUDGET_OVERHEAD_CHARS: usize = 1_200;

#[derive(Clone, Debug)]
pub(crate) struct CodeRef {
    pub(crate) rel_path: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) expected_sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeRefOpenResult {
    pub(crate) normalized_ref: String,
    pub(crate) rel_path: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) sha256: String,
    pub(crate) expected_sha256: Option<String>,
    pub(crate) stale: bool,
    pub(crate) content: String,
    pub(crate) lines_returned: usize,
    pub(crate) reached_eof: bool,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeRefError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) recovery: Option<String>,
}

impl CodeRefError {
    pub(crate) fn invalid_input(message: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self {
            code: "INVALID_INPUT",
            message: message.into(),
            recovery: Some(recovery.into()),
        }
    }

    pub(crate) fn unavailable(message: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self {
            code: "CODE_REF_UNAVAILABLE",
            message: message.into(),
            recovery: Some(recovery.into()),
        }
    }

    pub(crate) fn io(message: impl Into<String>) -> Self {
        Self {
            code: "IO_ERROR",
            message: message.into(),
            recovery: Some("Ensure the file exists and is readable.".to_string()),
        }
    }
}

pub(crate) fn maybe_open_code_ref(
    storage_dir: &Path,
    raw_ref: &str,
    max_chars: usize,
) -> Option<Result<CodeRefOpenResult, CodeRefError>> {
    if !starts_with_case_insensitive(raw_ref.trim(), CODE_REF_PREFIX) {
        return None;
    }
    Some(open_code_ref(storage_dir, raw_ref, max_chars))
}

fn open_code_ref(
    storage_dir: &Path,
    raw_ref: &str,
    max_chars: usize,
) -> Result<CodeRefOpenResult, CodeRefError> {
    let parsed = parse_code_ref(raw_ref)?;

    let repo_root = repo_root_from_storage_dir_strict(storage_dir).ok_or_else(|| {
        CodeRefError::unavailable(
            "code refs require repo-local storage",
            "Run BranchMind with repo-local storage at <repo>/.agents/mcp/.branchmind (the default when launched inside a repo).",
        )
    })?;

    let repo_root = std::fs::canonicalize(&repo_root).unwrap_or_else(|_| repo_root.to_path_buf());

    let rel_path = parsed.rel_path.clone();
    let joined = repo_root.join(PathBuf::from(&rel_path));
    if let Ok(meta) = std::fs::symlink_metadata(&joined)
        && meta.file_type().is_symlink()
    {
        return Err(CodeRefError::invalid_input(
            "code ref path must not be a symlink",
            "Point to a real file inside the repo (symlinks are rejected).",
        ));
    }
    let file_path = std::fs::canonicalize(&joined).map_err(|err| {
        CodeRefError::io(format!(
            "unable to open code ref path: {} ({})",
            rel_path, err
        ))
    })?;
    if !file_path.starts_with(&repo_root) {
        return Err(CodeRefError::invalid_input(
            "code ref path escapes the repo root",
            "Use a path inside the repo (no '..' traversal; symlinks outside the repo are rejected).",
        ));
    }
    let meta = std::fs::metadata(&file_path)
        .map_err(|err| CodeRefError::io(format!("unable to read metadata ({err})")))?;
    if !meta.is_file() {
        return Err(CodeRefError::invalid_input(
            "code ref path must point to a file",
            "Pick a repo file path (directories are not supported).",
        ));
    }

    let content_budget = max_chars
        .saturating_sub(CODE_REF_BUDGET_OVERHEAD_CHARS)
        .max(2_000);
    let (content, lines_returned, reached_eof, content_truncated) = read_numbered_lines(
        &file_path,
        parsed.start_line,
        parsed.end_line,
        content_budget,
    )
    .map_err(|err| CodeRefError::io(err.to_string()))?;

    let sha256 = sha256_hex(content.as_bytes());
    let normalized_ref = format!(
        "{CODE_REF_PREFIX}{path}#L{start}-L{end}{CODE_REF_HASH_TAG}{sha}",
        path = rel_path,
        start = parsed.start_line,
        end = parsed.end_line,
        sha = sha256
    );

    let stale = parsed
        .expected_sha256
        .as_deref()
        .is_some_and(|expected| !expected.eq_ignore_ascii_case(&sha256));

    Ok(CodeRefOpenResult {
        normalized_ref,
        rel_path,
        start_line: parsed.start_line,
        end_line: parsed.end_line,
        sha256,
        expected_sha256: parsed.expected_sha256,
        stale,
        content,
        lines_returned,
        reached_eof,
        truncated: content_truncated,
    })
}

fn parse_code_ref(raw: &str) -> Result<CodeRef, CodeRefError> {
    let raw = raw.trim();
    if !starts_with_case_insensitive(raw, CODE_REF_PREFIX) {
        return Err(CodeRefError::invalid_input(
            "code ref must start with code:",
            "Use code:<relative/path>#L<start>-L<end>[@sha256:<hex>].",
        ));
    }

    let rest = raw[CODE_REF_PREFIX.len()..].trim();
    if rest.is_empty() {
        return Err(CodeRefError::invalid_input(
            "code ref path is required",
            "Use code:<relative/path>#L<start>-L<end>.",
        ));
    }

    let (rest, expected_sha256) = split_sha_suffix(rest)?;
    let (path_part_raw, frag_raw) = rest.split_once('#').unwrap_or((rest, ""));
    let mut rel_path = normalize_rel_path(path_part_raw)?;
    if rel_path.is_empty() {
        return Err(CodeRefError::invalid_input(
            "code ref path must not be empty",
            "Use code:<relative/path>#L<start>-L<end>.",
        ));
    }
    // Prevent accidental `code:foo` from opening ambiguous ranges.
    if frag_raw.trim().is_empty() {
        return Err(CodeRefError::invalid_input(
            "code ref line range is required",
            "Use code:<relative/path>#L<start>-L<end> (e.g. code:src/lib.rs#L10-L42).",
        ));
    }

    let (start_line, end_line) = parse_line_fragment(frag_raw)?;
    let lines = end_line.saturating_sub(start_line).saturating_add(1);
    if lines > CODE_REF_MAX_LINES {
        return Err(CodeRefError::invalid_input(
            format!("code ref range too large ({lines} lines)"),
            format!("Pick at most {CODE_REF_MAX_LINES} lines per code ref."),
        ));
    }

    // Canonicalize path formatting (avoid accidental platform separators).
    rel_path = rel_path.replace('\\', "/");

    Ok(CodeRef {
        rel_path,
        start_line,
        end_line,
        expected_sha256,
    })
}

fn split_sha_suffix(rest: &str) -> Result<(&str, Option<String>), CodeRefError> {
    let lower = rest.to_ascii_lowercase();
    let marker = CODE_REF_HASH_TAG;
    let Some(pos) = lower.rfind(marker) else {
        return Ok((rest, None));
    };
    let (before, after_marker) = rest.split_at(pos);
    let after_marker = &after_marker[marker.len()..];
    let sha_raw = after_marker.trim();
    if sha_raw.is_empty() {
        return Err(CodeRefError::invalid_input(
            "code ref sha256 suffix is empty",
            "Use @sha256:<hex> (64 lowercase hex chars) or omit the suffix.",
        ));
    }
    if !sha_raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CodeRefError::invalid_input(
            "code ref sha256 must be hex",
            "Use @sha256:<hex> (64 lowercase hex chars) or omit the suffix.",
        ));
    }
    if sha_raw.len() != 64 {
        return Err(CodeRefError::invalid_input(
            format!(
                "code ref sha256 must be 64 hex chars (got {})",
                sha_raw.len()
            ),
            "Use @sha256:<64-hex> or omit the suffix.",
        ));
    }
    Ok((
        before.trim_end_matches('@').trim(),
        Some(sha_raw.to_ascii_lowercase()),
    ))
}

fn normalize_rel_path(raw: &str) -> Result<String, CodeRefError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }
    if raw.starts_with('/') {
        return Err(CodeRefError::invalid_input(
            "code ref path must be relative",
            "Drop the leading '/' and use a repo-relative path.",
        ));
    }
    let raw = raw.trim_start_matches("./");
    let raw = raw.replace('\\', "/");

    let path = Path::new(&raw);
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(CodeRefError::invalid_input(
                    "code ref path must be relative",
                    "Use a repo-relative path like crates/mcp/src/main.rs.",
                ));
            }
            Component::ParentDir => {
                return Err(CodeRefError::invalid_input(
                    "code ref path must not contain '..'",
                    "Use a path inside the repo root (no traversal).",
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(raw)
}

fn parse_line_fragment(fragment: &str) -> Result<(usize, usize), CodeRefError> {
    let frag = fragment.trim();
    let Some(first) = frag.chars().next() else {
        return Err(CodeRefError::invalid_input(
            "invalid code ref line fragment",
            "Use #L<start>-L<end> (e.g. #L10-L42).",
        ));
    };
    if frag.len() < 2 || !first.eq_ignore_ascii_case(&'l') {
        return Err(CodeRefError::invalid_input(
            "invalid code ref line fragment",
            "Use #L<start>-L<end> (e.g. #L10-L42).",
        ));
    }
    let frag_lower = frag.to_ascii_lowercase();
    let after_l = &frag[1..];
    if let Some(pos) = frag_lower.find("-l") {
        let start_raw = frag[1..pos].trim();
        let end_raw = frag[pos + 2..].trim();
        let start = parse_positive_usize(start_raw, "start_line")?;
        let end = parse_positive_usize(end_raw, "end_line")?;
        if end < start {
            return Err(CodeRefError::invalid_input(
                "end_line must be >= start_line",
                "Use #L<start>-L<end> where end >= start.",
            ));
        }
        Ok((start, end))
    } else {
        let start = parse_positive_usize(after_l.trim(), "line")?;
        Ok((start, start))
    }
}

fn parse_positive_usize(raw: &str, label: &str) -> Result<usize, CodeRefError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CodeRefError::invalid_input(
            format!("{label}: missing"),
            "Use positive 1-based line numbers.",
        ));
    }
    let parsed = raw.parse::<usize>().map_err(|_| {
        CodeRefError::invalid_input(format!("{label}: invalid integer"), "Use digits only.")
    })?;
    if parsed == 0 {
        return Err(CodeRefError::invalid_input(
            format!("{label}: must be >= 1"),
            "Use positive 1-based line numbers.",
        ));
    }
    Ok(parsed)
}

fn read_numbered_lines(
    file_path: &Path,
    start_line: usize,
    end_line: usize,
    max_chars: usize,
) -> std::io::Result<(String, usize, bool, bool)> {
    let file = std::fs::File::open(file_path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut out = String::new();

    let mut line_no: usize = 0;
    let mut returned: usize = 0;
    let mut reached_eof = false;
    let mut truncated = false;

    let mut buf = String::new();
    while line_no < end_line {
        buf.clear();
        let read = reader.read_line(&mut buf)?;
        if read == 0 {
            reached_eof = true;
            break;
        }
        line_no += 1;
        if line_no < start_line {
            continue;
        }

        // Normalize CRLF to LF for deterministic hashing/rendering.
        if buf.ends_with("\r\n") {
            buf.truncate(buf.len().saturating_sub(2));
        } else if buf.ends_with('\n') {
            buf.truncate(buf.len().saturating_sub(1));
        }

        let _ = writeln!(&mut out, "{line_no:>5} | {line}", line = buf);
        returned += 1;

        if out.len() >= max_chars {
            truncated = true;
            break;
        }
    }

    Ok((out.trim_end().to_string(), returned, reached_eof, truncated))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.as_slice() {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn starts_with_case_insensitive(haystack: &str, prefix: &str) -> bool {
    haystack
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}
