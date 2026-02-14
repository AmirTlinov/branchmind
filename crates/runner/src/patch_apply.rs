#![forbid(unsafe_code)]

use super::patch_types::{ApplyError, ApplyResult, PatchOp, WriterPatchPack};
use std::path::Path;

/// Validate all paths in the patch pack for traversal attacks.
pub(crate) fn validate_paths(pack: &WriterPatchPack) -> Result<(), ApplyError> {
    for file_patch in &pack.patches {
        validate_single_path(&file_patch.path)?;
    }
    Ok(())
}

fn validate_single_path(path: &str) -> Result<(), ApplyError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(ApplyError::PathTraversal {
            path: path.to_string(),
        });
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ApplyError::PathTraversal {
            path: path.to_string(),
        });
    }
    if path.contains("..") {
        return Err(ApplyError::PathTraversal {
            path: path.to_string(),
        });
    }
    Ok(())
}

/// Find the first contiguous match of `needle_lines` in `haystack_lines`.
/// Returns the 0-based start index, or None.
pub(crate) fn find_contiguous_match(
    haystack_lines: &[String],
    needle_lines: &[String],
) -> Option<usize> {
    if needle_lines.is_empty() {
        return None;
    }
    if needle_lines.len() > haystack_lines.len() {
        return None;
    }

    let max_start = haystack_lines.len() - needle_lines.len();
    for start in 0..=max_start {
        let matched = needle_lines
            .iter()
            .enumerate()
            .all(|(i, needle)| haystack_lines[start + i] == *needle);
        if matched {
            return Some(start);
        }
    }
    None
}

/// Fuzzy match: trim whitespace from each line before comparing.
pub(crate) fn find_contiguous_match_fuzzy(
    haystack_lines: &[String],
    needle_lines: &[String],
) -> Option<usize> {
    if needle_lines.is_empty() || needle_lines.len() > haystack_lines.len() {
        return None;
    }

    let max_start = haystack_lines.len() - needle_lines.len();
    for start in 0..=max_start {
        let matched = needle_lines
            .iter()
            .enumerate()
            .all(|(i, needle)| haystack_lines[start + i].trim() == needle.trim());
        if matched {
            return Some(start);
        }
    }
    None
}

/// Apply all file patches to the repo root. Returns results for each file.
pub(crate) fn apply_patches(
    repo_root: &Path,
    pack: &WriterPatchPack,
) -> Result<ApplyResult, ApplyError> {
    validate_paths(pack)?;

    let mut files_modified = Vec::<String>::new();
    let mut files_created = Vec::<String>::new();
    let mut files_deleted = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();

    for file_patch in &pack.patches {
        let rel_path = file_patch.path.trim();
        let abs_path = repo_root.join(rel_path);

        for op in &file_patch.ops {
            match op {
                PatchOp::CreateFile { content } => {
                    if abs_path.exists() {
                        return Err(ApplyError::FileAlreadyExists {
                            path: rel_path.to_string(),
                        });
                    }
                    if let Some(parent) = abs_path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| ApplyError::IoError {
                            path: rel_path.to_string(),
                            message: e.to_string(),
                        })?;
                    }
                    let text = content.join("\n");
                    std::fs::write(&abs_path, &text).map_err(|e| ApplyError::IoError {
                        path: rel_path.to_string(),
                        message: e.to_string(),
                    })?;
                    files_created.push(rel_path.to_string());
                }
                PatchOp::DeleteFile => {
                    if !abs_path.exists() {
                        return Err(ApplyError::FileNotFound {
                            path: rel_path.to_string(),
                        });
                    }
                    std::fs::remove_file(&abs_path).map_err(|e| ApplyError::IoError {
                        path: rel_path.to_string(),
                        message: e.to_string(),
                    })?;
                    files_deleted.push(rel_path.to_string());
                }
                PatchOp::Replace {
                    old_lines,
                    new_lines,
                    anchor_ref,
                } => {
                    let (lines, modified) = apply_replace(
                        &abs_path,
                        rel_path,
                        old_lines,
                        new_lines,
                        anchor_ref.as_deref(),
                        &mut warnings,
                    )?;
                    std::fs::write(&abs_path, lines.join("\n")).map_err(|e| {
                        ApplyError::IoError {
                            path: rel_path.to_string(),
                            message: e.to_string(),
                        }
                    })?;
                    if modified && !files_modified.contains(&rel_path.to_string()) {
                        files_modified.push(rel_path.to_string());
                    }
                }
                PatchOp::InsertAfter { after, content } => {
                    let (lines, modified) =
                        apply_insert_after(&abs_path, rel_path, after, content, &mut warnings)?;
                    std::fs::write(&abs_path, lines.join("\n")).map_err(|e| {
                        ApplyError::IoError {
                            path: rel_path.to_string(),
                            message: e.to_string(),
                        }
                    })?;
                    if modified && !files_modified.contains(&rel_path.to_string()) {
                        files_modified.push(rel_path.to_string());
                    }
                }
                PatchOp::InsertBefore { before, content } => {
                    let (lines, modified) =
                        apply_insert_before(&abs_path, rel_path, before, content, &mut warnings)?;
                    std::fs::write(&abs_path, lines.join("\n")).map_err(|e| {
                        ApplyError::IoError {
                            path: rel_path.to_string(),
                            message: e.to_string(),
                        }
                    })?;
                    if modified && !files_modified.contains(&rel_path.to_string()) {
                        files_modified.push(rel_path.to_string());
                    }
                }
            }
        }
    }

    Ok(ApplyResult {
        files_modified,
        files_created,
        files_deleted,
        warnings,
    })
}

fn read_file_lines(abs_path: &Path, rel_path: &str) -> Result<Vec<String>, ApplyError> {
    if !abs_path.exists() {
        return Err(ApplyError::FileNotFound {
            path: rel_path.to_string(),
        });
    }
    let content = std::fs::read_to_string(abs_path).map_err(|e| ApplyError::IoError {
        path: rel_path.to_string(),
        message: e.to_string(),
    })?;
    Ok(content.split('\n').map(|s| s.to_string()).collect())
}

fn apply_replace(
    abs_path: &Path,
    rel_path: &str,
    old_lines: &[String],
    new_lines: &[String],
    _anchor_ref: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<(Vec<String>, bool), ApplyError> {
    let mut lines = read_file_lines(abs_path, rel_path)?;

    // Try exact match first, then fuzzy.
    let pos = find_contiguous_match(&lines, old_lines).or_else(|| {
        let fuzzy = find_contiguous_match_fuzzy(&lines, old_lines);
        if fuzzy.is_some() {
            warnings.push(format!(
                "{rel_path}: used fuzzy match (whitespace-trimmed) for replace"
            ));
        }
        fuzzy
    });

    let Some(start) = pos else {
        let context = old_lines
            .first()
            .map(|s| s.chars().take(60).collect::<String>())
            .unwrap_or_default();
        return Err(ApplyError::MatchNotFound {
            path: rel_path.to_string(),
            context,
        });
    };

    // Replace: remove old_lines, insert new_lines.
    lines.splice(start..start + old_lines.len(), new_lines.iter().cloned());
    Ok((lines, true))
}

fn apply_insert_after(
    abs_path: &Path,
    rel_path: &str,
    after: &[String],
    content: &[String],
    warnings: &mut Vec<String>,
) -> Result<(Vec<String>, bool), ApplyError> {
    let mut lines = read_file_lines(abs_path, rel_path)?;

    let pos = find_contiguous_match(&lines, after).or_else(|| {
        let fuzzy = find_contiguous_match_fuzzy(&lines, after);
        if fuzzy.is_some() {
            warnings.push(format!("{rel_path}: used fuzzy match for insert_after"));
        }
        fuzzy
    });

    let Some(start) = pos else {
        let context = after
            .first()
            .map(|s| s.chars().take(60).collect::<String>())
            .unwrap_or_default();
        return Err(ApplyError::MatchNotFound {
            path: rel_path.to_string(),
            context,
        });
    };

    let insert_at = start + after.len();
    for (i, line) in content.iter().enumerate() {
        lines.insert(insert_at + i, line.clone());
    }

    Ok((lines, true))
}

fn apply_insert_before(
    abs_path: &Path,
    rel_path: &str,
    before: &[String],
    content: &[String],
    warnings: &mut Vec<String>,
) -> Result<(Vec<String>, bool), ApplyError> {
    let mut lines = read_file_lines(abs_path, rel_path)?;

    let pos = find_contiguous_match(&lines, before).or_else(|| {
        let fuzzy = find_contiguous_match_fuzzy(&lines, before);
        if fuzzy.is_some() {
            warnings.push(format!("{rel_path}: used fuzzy match for insert_before"));
        }
        fuzzy
    });

    let Some(start) = pos else {
        let context = before
            .first()
            .map(|s| s.chars().take(60).collect::<String>())
            .unwrap_or_default();
        return Err(ApplyError::MatchNotFound {
            path: rel_path.to_string(),
            context,
        });
    };

    for (i, line) in content.iter().enumerate() {
        lines.insert(start + i, line.clone());
    }

    Ok((lines, true))
}

/// Rollback: restore files from a snapshot map.
pub(crate) fn rollback_files(
    repo_root: &Path,
    snapshots: &std::collections::HashMap<String, Option<String>>,
) {
    for (rel_path, original_content) in snapshots {
        let abs_path = repo_root.join(rel_path);
        match original_content {
            Some(content) => {
                let _ = std::fs::write(&abs_path, content);
            }
            None => {
                // File was created by patches — delete it.
                let _ = std::fs::remove_file(&abs_path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_types::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bm_patch_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replace_exact_match() {
        let dir = temp_dir("replace_exact");
        std::fs::write(
            dir.join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "test.rs".to_string(),
                ops: vec![PatchOp::Replace {
                    old_lines: vec!["    println!(\"hello\");".to_string()],
                    new_lines: vec!["    println!(\"world\");".to_string()],
                    anchor_ref: None,
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec!["test.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack).unwrap();
        assert_eq!(result.files_modified, vec!["test.rs"]);

        let content = std::fs::read_to_string(dir.join("test.rs")).unwrap();
        assert!(content.contains("println!(\"world\")"));
        assert!(!content.contains("println!(\"hello\")"));
    }

    #[test]
    fn replace_fuzzy_match() {
        let dir = temp_dir("replace_fuzzy");
        std::fs::write(
            dir.join("test.rs"),
            "fn main() {\n\tprintln!(\"hello\");\n}\n",
        )
        .unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "test.rs".to_string(),
                ops: vec![PatchOp::Replace {
                    old_lines: vec!["println!(\"hello\");".to_string()],
                    new_lines: vec!["    println!(\"world\");".to_string()],
                    anchor_ref: None,
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec!["test.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack).unwrap();
        assert_eq!(result.files_modified, vec!["test.rs"]);
        assert!(!result.warnings.is_empty()); // fuzzy match warning
    }

    #[test]
    fn insert_after() {
        let dir = temp_dir("insert_after");
        std::fs::write(dir.join("test.rs"), "use std::io;\n\nfn main() {}\n").unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "test.rs".to_string(),
                ops: vec![PatchOp::InsertAfter {
                    after: vec!["use std::io;".to_string()],
                    content: vec!["use std::path::Path;".to_string()],
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec!["test.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack).unwrap();
        assert_eq!(result.files_modified, vec!["test.rs"]);

        let content = std::fs::read_to_string(dir.join("test.rs")).unwrap();
        assert!(content.contains("use std::path::Path;"));
    }

    #[test]
    fn insert_before() {
        let dir = temp_dir("insert_before");
        std::fs::write(dir.join("test.rs"), "fn main() {\n    run();\n}\n").unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "test.rs".to_string(),
                ops: vec![PatchOp::InsertBefore {
                    before: vec!["fn main() {".to_string()],
                    content: vec!["// Entry point".to_string()],
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec!["test.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let _result = apply_patches(&dir, &pack).unwrap();
        let content = std::fs::read_to_string(dir.join("test.rs")).unwrap();
        assert!(content.starts_with("// Entry point"));
    }

    #[test]
    fn create_file() {
        let dir = temp_dir("create_file");

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "new_file.rs".to_string(),
                ops: vec![PatchOp::CreateFile {
                    content: vec!["fn new() {}".to_string()],
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec!["new_file.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack).unwrap();
        assert_eq!(result.files_created, vec!["new_file.rs"]);
        assert!(dir.join("new_file.rs").exists());
    }

    #[test]
    fn delete_file() {
        let dir = temp_dir("delete_file");
        std::fs::write(dir.join("remove_me.rs"), "old").unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "remove_me.rs".to_string(),
                ops: vec![PatchOp::DeleteFile],
            }],
            summary: "test".to_string(),
            affected_files: vec!["remove_me.rs".to_string()],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack).unwrap();
        assert_eq!(result.files_deleted, vec!["remove_me.rs"]);
        assert!(!dir.join("remove_me.rs").exists());
    }

    #[test]
    fn missing_old_lines_fails() {
        let dir = temp_dir("missing_match");
        std::fs::write(dir.join("test.rs"), "fn main() {}\n").unwrap();

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "test.rs".to_string(),
                ops: vec![PatchOp::Replace {
                    old_lines: vec!["fn nonexistent() {}".to_string()],
                    new_lines: vec!["fn replaced() {}".to_string()],
                    anchor_ref: None,
                }],
            }],
            summary: "test".to_string(),
            affected_files: vec![],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ApplyError::MatchNotFound { .. }));
    }

    #[test]
    fn path_traversal_rejected() {
        let dir = temp_dir("traversal");

        let pack = WriterPatchPack {
            slice_id: "s1".to_string(),
            patches: vec![FilePatch {
                path: "../etc/passwd".to_string(),
                ops: vec![PatchOp::DeleteFile],
            }],
            summary: "test".to_string(),
            affected_files: vec![],
            checks_to_run: vec![],
            insufficient_context: None,
        };

        let result = apply_patches(&dir, &pack);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ApplyError::PathTraversal { .. }
        ));
    }

    #[test]
    fn rollback_restores_files() {
        let dir = temp_dir("rollback");
        std::fs::write(dir.join("a.rs"), "original").unwrap();

        // Modify
        std::fs::write(dir.join("a.rs"), "modified").unwrap();
        std::fs::write(dir.join("new.rs"), "created").unwrap();

        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("a.rs".to_string(), Some("original".to_string()));
        snapshots.insert("new.rs".to_string(), None); // was created

        rollback_files(&dir, &snapshots);

        assert_eq!(
            std::fs::read_to_string(dir.join("a.rs")).unwrap(),
            "original"
        );
        assert!(!dir.join("new.rs").exists());
    }
}
