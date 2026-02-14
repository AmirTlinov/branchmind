#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WriterPatchPack {
    pub slice_id: String,
    pub patches: Vec<FilePatch>,
    pub summary: String,
    pub affected_files: Vec<String>,
    pub checks_to_run: Vec<String>,
    pub insufficient_context: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FilePatch {
    pub path: String,
    pub ops: Vec<PatchOp>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PatchOp {
    Replace {
        old_lines: Vec<String>,
        new_lines: Vec<String>,
        #[serde(default)]
        anchor_ref: Option<String>,
    },
    InsertAfter {
        after: Vec<String>,
        content: Vec<String>,
    },
    InsertBefore {
        before: Vec<String>,
        content: Vec<String>,
    },
    CreateFile {
        content: Vec<String>,
    },
    DeleteFile,
}

#[derive(Clone, Debug)]
pub(crate) enum ApplyError {
    PathTraversal { path: String },
    FileNotFound { path: String },
    FileAlreadyExists { path: String },
    MatchNotFound { path: String, context: String },
    IoError { path: String, message: String },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathTraversal { path } => write!(f, "path traversal rejected: {path}"),
            Self::FileNotFound { path } => write!(f, "file not found: {path}"),
            Self::FileAlreadyExists { path } => write!(f, "file already exists: {path}"),
            Self::MatchNotFound { path, context } => {
                write!(f, "match not found in {path}: {context}")
            }
            Self::IoError { path, message } => write!(f, "io error on {path}: {message}"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ApplyResult {
    pub files_modified: Vec<String>,
    pub files_created: Vec<String>,
    pub files_deleted: Vec<String>,
    pub warnings: Vec<String>,
}
