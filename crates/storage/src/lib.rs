#![forbid(unsafe_code)]

use bm_core::graph::{
    ConflictId, GraphNodeId, GraphRel, GraphTagError, GraphType,
    normalize_tags as core_normalize_tags,
};
use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use bm_core::paths::StepPath;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde_json::{Value as JsonValue, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const DEFAULT_BRANCH: &str = "main";

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
    RevisionMismatch {
        expected: i64,
        actual: i64,
    },
    UnknownId,
    UnknownBranch,
    UnknownConflict,
    ConflictAlreadyResolved,
    MergeNotSupported,
    BranchAlreadyExists,
    BranchCycle,
    BranchDepthExceeded,
    StepNotFound,
    CheckpointsNotConfirmed {
        criteria: bool,
        tests: bool,
        security: bool,
        perf: bool,
        docs: bool,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::RevisionMismatch { expected, actual } => {
                write!(
                    f,
                    "revision mismatch (expected={expected}, actual={actual})"
                )
            }
            Self::UnknownId => write!(f, "unknown id"),
            Self::UnknownBranch => write!(f, "unknown branch"),
            Self::UnknownConflict => write!(f, "unknown conflict"),
            Self::ConflictAlreadyResolved => write!(f, "conflict already resolved"),
            Self::MergeNotSupported => write!(f, "merge not supported"),
            Self::BranchAlreadyExists => write!(f, "branch already exists"),
            Self::BranchCycle => write!(f, "branch base cycle"),
            Self::BranchDepthExceeded => write!(f, "branch base depth exceeded"),
            Self::StepNotFound => write!(f, "step not found"),
            Self::CheckpointsNotConfirmed {
                criteria,
                tests,
                security,
                perf,
                docs,
            } => {
                write!(
                    f,
                    "checkpoints not confirmed (criteria={criteria}, tests={tests}, security={security}, perf={perf}, docs={docs})"
                )
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Clone, Debug)]
pub struct PlanRow {
    pub id: String,
    pub revision: i64,
    pub title: String,
    pub contract: Option<String>,
    pub contract_json: Option<String>,
    pub description: Option<String>,
    pub context: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub plan_doc: Option<String>,
    pub plan_current: i64,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub criteria_auto_confirmed: bool,
    pub tests_auto_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlanChecklist {
    pub doc: Option<String>,
    pub current: i64,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub id: String,
    pub revision: i64,
    pub parent_plan_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub assignee: Option<String>,
    pub domain: Option<String>,
    pub phase: Option<String>,
    pub component: Option<String>,
    pub context: Option<String>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub criteria_auto_confirmed: bool,
    pub tests_auto_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct ReasoningRefRow {
    pub branch: String,
    pub notes_doc: String,
    pub graph_doc: String,
    pub trace_doc: String,
}

#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub base_branch: Option<String>,
    pub base_seq: Option<i64>,
    pub created_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Notes,
    Trace,
    Graph,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Trace => "trace",
            Self::Graph => "graph",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocEntryKind {
    Note,
    Event,
}

impl DocEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Event => "event",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DocEntryRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub branch: String,
    pub doc: String,
    pub kind: DocEntryKind,
    pub title: Option<String>,
    pub format: Option<String>,
    pub meta_json: Option<String>,
    pub content: Option<String>,
    pub source_event_id: Option<String>,
    pub event_type: Option<String>,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub payload_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocSlice {
    pub entries: Vec<DocEntryRow>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct DocumentRow {
    pub branch: String,
    pub doc: String,
    pub kind: DocumentKind,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct VcsRefRow {
    pub reference: String,
    pub branch: String,
    pub doc: String,
    pub seq: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct VcsRefUpdate {
    pub reference: VcsRefRow,
    pub old_seq: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct VcsTagRow {
    pub name: String,
    pub branch: String,
    pub doc: String,
    pub seq: i64,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct VcsReflogRow {
    pub reference: String,
    pub branch: String,
    pub doc: String,
    pub old_seq: Option<i64>,
    pub new_seq: i64,
    pub ts_ms: i64,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MergeNotesResult {
    pub merged: usize,
    pub skipped: usize,
    pub count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct ThinkCardInput {
    pub card_id: String,
    pub card_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub meta_json: Option<String>,
    pub content: String,
    pub payload_json: String,
}

#[derive(Clone, Debug)]
pub struct ThinkCardCommitResult {
    pub inserted: bool,
    pub nodes_upserted: usize,
    pub edges_upserted: usize,
    pub last_seq: Option<i64>,
}

pub use bm_core::graph::{
    GraphApplyResult, GraphConflictDetail, GraphConflictResolveResult, GraphConflictSummary,
    GraphDiffChange, GraphDiffSlice, GraphEdge, GraphEdgeUpsert, GraphMergeResult, GraphNode,
    GraphNodeUpsert, GraphOp, GraphQueryRequest, GraphQuerySlice, GraphValidateError,
    GraphValidateResult,
};

pub type GraphNodeRow = GraphNode;
pub type GraphEdgeRow = GraphEdge;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GraphEdgeKey {
    from: String,
    rel: String,
    to: String,
}

#[derive(Clone, Debug)]
enum GraphDiffCandidate {
    Node { to: GraphNodeRow },
    Edge { key: GraphEdgeKey, to: GraphEdgeRow },
}

impl GraphDiffCandidate {
    fn last_seq(&self) -> i64 {
        match self {
            Self::Node { to } => to.last_seq,
            Self::Edge { to, .. } => to.last_seq,
        }
    }
}

#[derive(Clone, Debug)]
enum GraphMergeCandidate {
    Node { theirs: GraphNodeRow },
    Edge { theirs: GraphEdgeRow },
}

impl GraphMergeCandidate {
    fn last_seq(&self) -> i64 {
        match self {
            Self::Node { theirs } => theirs.last_seq,
            Self::Edge { theirs } => theirs.last_seq,
        }
    }
}

#[derive(Clone, Debug)]
struct GraphConflictDetailRow {
    kind: String,
    key: String,
    from_branch: String,
    into_branch: String,
    doc: String,
    status: String,
    created_at_ms: i64,
    resolved_at_ms: Option<i64>,

    base_seq: Option<i64>,
    base_ts_ms: Option<i64>,
    base_deleted: Option<i64>,
    base_node_type: Option<String>,
    base_title: Option<String>,
    base_text: Option<String>,
    base_tags: Option<String>,
    base_status: Option<String>,
    base_meta_json: Option<String>,
    base_from_id: Option<String>,
    base_rel: Option<String>,
    base_to_id: Option<String>,
    base_edge_meta_json: Option<String>,

    theirs_seq: Option<i64>,
    theirs_ts_ms: Option<i64>,
    theirs_deleted: Option<i64>,
    theirs_node_type: Option<String>,
    theirs_title: Option<String>,
    theirs_text: Option<String>,
    theirs_tags: Option<String>,
    theirs_status: Option<String>,
    theirs_meta_json: Option<String>,
    theirs_from_id: Option<String>,
    theirs_rel: Option<String>,
    theirs_to_id: Option<String>,
    theirs_edge_meta_json: Option<String>,

    ours_seq: Option<i64>,
    ours_ts_ms: Option<i64>,
    ours_deleted: Option<i64>,
    ours_node_type: Option<String>,
    ours_title: Option<String>,
    ours_text: Option<String>,
    ours_tags: Option<String>,
    ours_status: Option<String>,
    ours_meta_json: Option<String>,
    ours_from_id: Option<String>,
    ours_rel: Option<String>,
    ours_to_id: Option<String>,
    ours_edge_meta_json: Option<String>,
}

impl GraphConflictDetailRow {
    fn into_detail(self, conflict_id: &str) -> GraphConflictDetail {
        let kind = self.kind.clone();
        let key = self.key.clone();

        let base_node = if kind == "node" && self.base_seq.is_some() {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.base_node_type.unwrap_or_default(),
                title: self.base_title,
                text: self.base_text,
                tags: decode_tags(self.base_tags.as_deref()),
                status: self.base_status,
                meta_json: self.base_meta_json,
                deleted: self.base_deleted.unwrap_or(0) != 0,
                last_seq: self.base_seq.unwrap_or(0),
                last_ts_ms: self.base_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let theirs_node = if kind == "node" && self.theirs_seq.unwrap_or(0) != 0 {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.theirs_node_type.unwrap_or_default(),
                title: self.theirs_title,
                text: self.theirs_text,
                tags: decode_tags(self.theirs_tags.as_deref()),
                status: self.theirs_status,
                meta_json: self.theirs_meta_json,
                deleted: self.theirs_deleted.unwrap_or(0) != 0,
                last_seq: self.theirs_seq.unwrap_or(0),
                last_ts_ms: self.theirs_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let ours_node = if kind == "node" && self.ours_seq.unwrap_or(0) != 0 {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.ours_node_type.unwrap_or_default(),
                title: self.ours_title,
                text: self.ours_text,
                tags: decode_tags(self.ours_tags.as_deref()),
                status: self.ours_status,
                meta_json: self.ours_meta_json,
                deleted: self.ours_deleted.unwrap_or(0) != 0,
                last_seq: self.ours_seq.unwrap_or(0),
                last_ts_ms: self.ours_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let base_edge = if kind == "edge" && self.base_seq.is_some() {
            match (self.base_from_id, self.base_rel, self.base_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.base_edge_meta_json,
                    deleted: self.base_deleted.unwrap_or(0) != 0,
                    last_seq: self.base_seq.unwrap_or(0),
                    last_ts_ms: self.base_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        let theirs_edge = if kind == "edge" && self.theirs_seq.unwrap_or(0) != 0 {
            match (self.theirs_from_id, self.theirs_rel, self.theirs_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.theirs_edge_meta_json,
                    deleted: self.theirs_deleted.unwrap_or(0) != 0,
                    last_seq: self.theirs_seq.unwrap_or(0),
                    last_ts_ms: self.theirs_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        let ours_edge = if kind == "edge" && self.ours_seq.unwrap_or(0) != 0 {
            match (self.ours_from_id, self.ours_rel, self.ours_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.ours_edge_meta_json,
                    deleted: self.ours_deleted.unwrap_or(0) != 0,
                    last_seq: self.ours_seq.unwrap_or(0),
                    last_ts_ms: self.ours_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        GraphConflictDetail {
            conflict_id: conflict_id.to_string(),
            kind,
            key,
            from_branch: self.from_branch,
            into_branch: self.into_branch,
            doc: self.doc,
            status: self.status,
            created_at_ms: self.created_at_ms,
            resolved_at_ms: self.resolved_at_ms,
            base_node,
            theirs_node,
            ours_node,
            base_edge,
            theirs_edge,
            ours_edge,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StepRef {
    pub step_id: String,
    pub path: String,
}

#[derive(Clone, Debug)]
enum OpsHistoryTarget {
    Task { title: Option<String> },
    Step { step: StepRef },
    TaskNode,
}

#[derive(Clone, Debug)]
pub struct StepOpResult {
    pub task_revision: i64,
    pub step: StepRef,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct StepCloseResult {
    pub task_revision: i64,
    pub step: StepRef,
    pub events: Vec<EventRow>,
}

#[derive(Clone, Debug)]
pub struct DecomposeResult {
    pub task_revision: i64,
    pub steps: Vec<StepRef>,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct TaskDetailPatch {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub priority: Option<String>,
    pub contract: Option<Option<String>>,
    pub contract_json: Option<Option<String>>,
    pub domain: Option<Option<String>>,
    pub phase: Option<Option<String>>,
    pub component: Option<Option<String>>,
    pub assignee: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct StepPatch {
    pub title: Option<String>,
    pub success_criteria: Option<Vec<String>>,
    pub tests: Option<Vec<String>>,
    pub blockers: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct TaskNodePatch {
    pub title: Option<String>,
    pub status: Option<String>,
    pub status_manual: Option<bool>,
    pub priority: Option<String>,
    pub blocked: Option<bool>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub blockers: Option<Vec<String>>,
    pub dependencies: Option<Vec<String>>,
    pub next_steps: Option<Vec<String>>,
    pub problems: Option<Vec<String>>,
    pub risks: Option<Vec<String>>,
    pub success_criteria: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct TaskNodeItems {
    pub blockers: Vec<String>,
    pub dependencies: Vec<String>,
    pub next_steps: Vec<String>,
    pub problems: Vec<String>,
    pub risks: Vec<String>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EvidenceArtifactInput {
    pub kind: String,
    pub command: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i64>,
    pub diff: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub external_uri: Option<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewStep {
    pub title: String,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StepStatus {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub completed: bool,
}

#[derive(Clone, Debug)]
pub struct TaskStepSummary {
    pub total_steps: i64,
    pub completed_steps: i64,
    pub open_steps: i64,
    pub missing_criteria: i64,
    pub missing_tests: i64,
    pub missing_security: i64,
    pub missing_perf: i64,
    pub missing_docs: i64,
    pub first_open: Option<StepStatus>,
}

#[derive(Clone, Debug)]
pub struct StepDetail {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub completed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StepListRow {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub completed: bool,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskNodeRow {
    pub node_id: String,
    pub task_id: String,
    pub parent_step_id: String,
    pub ordinal: i64,
    pub title: String,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub description: Option<String>,
    pub context: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct TaskNodeDetail {
    pub row: TaskNodeRow,
    pub path: String,
    pub blockers: Vec<String>,
    pub dependencies: Vec<String>,
    pub next_steps: Vec<String>,
    pub problems: Vec<String>,
    pub risks: Vec<String>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskNodeRef {
    pub node_id: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct TaskNodeOpResult {
    pub task_revision: i64,
    pub node: TaskNodeRef,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct EvidenceCaptureResult {
    pub revision: i64,
    pub step: Option<StepRef>,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct EventRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub event_type: String,
    pub payload_json: String,
}

impl EventRow {
    pub fn event_id(&self) -> String {
        format!("evt_{:016}", self.seq)
    }
}

#[derive(Clone, Debug)]
pub struct OpsHistoryRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub intent: String,
    pub payload_json: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub undoable: bool,
    pub undone: bool,
}

#[derive(Debug)]
pub struct SqliteStore {
    storage_dir: PathBuf,
    conn: Connection,
}

impl SqliteStore {
    pub fn open(storage_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&storage_dir)?;
        let db_path = storage_dir.join("branchmind_rust.db");
        let conn = Connection::open(db_path)?;
        let store = Self { storage_dir, conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub fn default_branch_name(&self) -> &'static str {
        DEFAULT_BRANCH
    }

    pub fn next_card_id(&mut self, workspace: &WorkspaceId) -> Result<String, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        let seq = next_counter_tx(&tx, workspace.as_str(), "card_seq")?;
        tx.commit()?;
        Ok(format!("CARD-{seq}"))
    }

    pub fn workspace_init(&mut self, workspace: &WorkspaceId) -> Result<(), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        let _ = bootstrap_default_branch_tx(&tx, workspace.as_str(), now_ms)?;
        let _ = ensure_checkout_branch_tx(&tx, workspace.as_str(), DEFAULT_BRANCH, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workspaces (
              workspace TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS counters (
              workspace TEXT NOT NULL,
              name TEXT NOT NULL,
              value INTEGER NOT NULL,
              PRIMARY KEY (workspace, name)
            );

            CREATE TABLE IF NOT EXISTS plans (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              revision INTEGER NOT NULL,
              title TEXT NOT NULL,
              contract TEXT,
              contract_json TEXT,
              description TEXT,
              context TEXT,
              status TEXT NOT NULL DEFAULT 'TODO',
              status_manual INTEGER NOT NULL DEFAULT 0,
              priority TEXT NOT NULL DEFAULT 'MEDIUM',
              plan_doc TEXT,
              plan_current INTEGER NOT NULL DEFAULT 0,
              criteria_confirmed INTEGER NOT NULL DEFAULT 0,
              tests_confirmed INTEGER NOT NULL DEFAULT 0,
              criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
              tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
              security_confirmed INTEGER NOT NULL DEFAULT 0,
              perf_confirmed INTEGER NOT NULL DEFAULT 0,
              docs_confirmed INTEGER NOT NULL DEFAULT 0,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE TABLE IF NOT EXISTS tasks (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              revision INTEGER NOT NULL,
              parent_plan_id TEXT NOT NULL,
              title TEXT NOT NULL,
              description TEXT,
              status TEXT NOT NULL DEFAULT 'TODO',
              status_manual INTEGER NOT NULL DEFAULT 0,
              priority TEXT NOT NULL DEFAULT 'MEDIUM',
              blocked INTEGER NOT NULL DEFAULT 0,
              assignee TEXT,
              domain TEXT,
              phase TEXT,
              component TEXT,
              context TEXT,
              criteria_confirmed INTEGER NOT NULL DEFAULT 0,
              tests_confirmed INTEGER NOT NULL DEFAULT 0,
              criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
              tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
              security_confirmed INTEGER NOT NULL DEFAULT 0,
              perf_confirmed INTEGER NOT NULL DEFAULT 0,
              docs_confirmed INTEGER NOT NULL DEFAULT 0,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE TABLE IF NOT EXISTS events (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              task_id TEXT,
              path TEXT,
              type TEXT NOT NULL,
              payload_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS focus (
              workspace TEXT PRIMARY KEY,
              focus_id TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reasoning_refs (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              kind TEXT NOT NULL,
              branch TEXT NOT NULL,
              notes_doc TEXT NOT NULL,
              graph_doc TEXT NOT NULL,
              trace_doc TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE TABLE IF NOT EXISTS documents (
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              kind TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, branch, doc)
            );

            CREATE TABLE IF NOT EXISTS doc_entries (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              kind TEXT NOT NULL,
              title TEXT,
              format TEXT,
              meta_json TEXT,
              content TEXT,
              source_event_id TEXT,
              event_type TEXT,
              task_id TEXT,
              path TEXT,
              payload_json TEXT
            );

            CREATE TABLE IF NOT EXISTS vcs_refs (
              workspace TEXT NOT NULL,
              ref TEXT NOT NULL,
              doc TEXT NOT NULL,
              branch TEXT NOT NULL,
              seq INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, ref, doc)
            );

            CREATE TABLE IF NOT EXISTS vcs_reflog (
              workspace TEXT NOT NULL,
              ref TEXT NOT NULL,
              doc TEXT NOT NULL,
              branch TEXT NOT NULL,
              old_seq INTEGER,
              new_seq INTEGER NOT NULL,
              message TEXT,
              ts_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, ref, doc, ts_ms, new_seq)
            );

            CREATE TABLE IF NOT EXISTS vcs_tags (
              workspace TEXT NOT NULL,
              name TEXT NOT NULL,
              doc TEXT NOT NULL,
              branch TEXT NOT NULL,
              seq INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, name)
            );

            CREATE TABLE IF NOT EXISTS branches (
              workspace TEXT NOT NULL,
              name TEXT NOT NULL,
              base_branch TEXT NOT NULL,
              base_seq INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, name)
            );

            CREATE TABLE IF NOT EXISTS branch_checkout (
              workspace TEXT PRIMARY KEY,
              branch TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS steps (
              workspace TEXT NOT NULL,
              task_id TEXT NOT NULL,
              step_id TEXT NOT NULL,
              parent_step_id TEXT,
              ordinal INTEGER NOT NULL,
              title TEXT NOT NULL,
              completed INTEGER NOT NULL,
              completed_at_ms INTEGER,
              started_at_ms INTEGER,
              criteria_confirmed INTEGER NOT NULL,
              tests_confirmed INTEGER NOT NULL,
              criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
              tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
              security_confirmed INTEGER NOT NULL DEFAULT 0,
              perf_confirmed INTEGER NOT NULL DEFAULT 0,
              docs_confirmed INTEGER NOT NULL DEFAULT 0,
              blocked INTEGER NOT NULL DEFAULT 0,
              block_reason TEXT,
              verification_outcome TEXT,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, step_id)
            );

            CREATE TABLE IF NOT EXISTS step_criteria (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_tests (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_blockers (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_notes (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              task_id TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              note TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plan_checklist (
              workspace TEXT NOT NULL,
              plan_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, plan_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS task_items (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              field TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, entity_kind, entity_id, field, ordinal)
            );

            CREATE TABLE IF NOT EXISTS task_nodes (
              workspace TEXT NOT NULL,
              node_id TEXT NOT NULL,
              task_id TEXT NOT NULL,
              parent_step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              status_manual INTEGER NOT NULL DEFAULT 0,
              priority TEXT NOT NULL DEFAULT 'MEDIUM',
              blocked INTEGER NOT NULL DEFAULT 0,
              description TEXT,
              context TEXT,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, node_id)
            );

            CREATE TABLE IF NOT EXISTS evidence_artifacts (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              kind TEXT NOT NULL,
              command TEXT,
              stdout TEXT,
              stderr TEXT,
              exit_code INTEGER,
              diff TEXT,
              content TEXT,
              url TEXT,
              external_uri TEXT,
              meta_json TEXT,
              PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS evidence_checks (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              check_text TEXT NOT NULL,
              PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS evidence_attachments (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              attachment TEXT NOT NULL,
              PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS checkpoint_notes (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              checkpoint TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              note TEXT NOT NULL,
              PRIMARY KEY (workspace, entity_kind, entity_id, checkpoint, ordinal)
            );

            CREATE TABLE IF NOT EXISTS checkpoint_evidence (
              workspace TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              checkpoint TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              ref TEXT NOT NULL,
              PRIMARY KEY (workspace, entity_kind, entity_id, checkpoint, ordinal)
            );

            CREATE TABLE IF NOT EXISTS ops_history (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              task_id TEXT,
              path TEXT,
              intent TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              before_json TEXT,
              after_json TEXT,
              undoable INTEGER NOT NULL DEFAULT 1,
              undone INTEGER NOT NULL DEFAULT 0,
              ts_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS graph_node_versions (
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              seq INTEGER NOT NULL,
              ts_ms INTEGER NOT NULL,
              node_id TEXT NOT NULL,
              node_type TEXT,
              title TEXT,
              text TEXT,
              tags TEXT,
              status TEXT,
              meta_json TEXT,
              deleted INTEGER NOT NULL,
              PRIMARY KEY (workspace, branch, doc, node_id, seq)
            );

            CREATE TABLE IF NOT EXISTS graph_edge_versions (
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              seq INTEGER NOT NULL,
              ts_ms INTEGER NOT NULL,
              from_id TEXT NOT NULL,
              rel TEXT NOT NULL,
              to_id TEXT NOT NULL,
              meta_json TEXT,
              deleted INTEGER NOT NULL,
              PRIMARY KEY (workspace, branch, doc, from_id, rel, to_id, seq)
            );

            CREATE TABLE IF NOT EXISTS graph_conflicts (
              workspace TEXT NOT NULL,
              conflict_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              key TEXT NOT NULL,
              from_branch TEXT NOT NULL,
              into_branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              base_cutoff_seq INTEGER NOT NULL,

              base_seq INTEGER,
              base_ts_ms INTEGER,
              base_deleted INTEGER,
              base_node_type TEXT,
              base_title TEXT,
              base_text TEXT,
              base_tags TEXT,
              base_status TEXT,
              base_meta_json TEXT,
              base_from_id TEXT,
              base_rel TEXT,
              base_to_id TEXT,
              base_edge_meta_json TEXT,

              theirs_seq INTEGER,
              theirs_ts_ms INTEGER,
              theirs_deleted INTEGER,
              theirs_node_type TEXT,
              theirs_title TEXT,
              theirs_text TEXT,
              theirs_tags TEXT,
              theirs_status TEXT,
              theirs_meta_json TEXT,
              theirs_from_id TEXT,
              theirs_rel TEXT,
              theirs_to_id TEXT,
              theirs_edge_meta_json TEXT,

              ours_seq INTEGER,
              ours_ts_ms INTEGER,
              ours_deleted INTEGER,
              ours_node_type TEXT,
              ours_title TEXT,
              ours_text TEXT,
              ours_tags TEXT,
              ours_status TEXT,
              ours_meta_json TEXT,
              ours_from_id TEXT,
              ours_rel TEXT,
              ours_to_id TEXT,
              ours_edge_meta_json TEXT,

              status TEXT NOT NULL,
              resolution TEXT,
              created_at_ms INTEGER NOT NULL,
              resolved_at_ms INTEGER,

              PRIMARY KEY (workspace, conflict_id)
            );

            CREATE INDEX IF NOT EXISTS idx_events_workspace_seq ON events(workspace, seq);
            CREATE INDEX IF NOT EXISTS idx_doc_entries_lookup ON doc_entries(workspace, branch, doc, seq);
            CREATE INDEX IF NOT EXISTS idx_doc_entries_workspace_seq ON doc_entries(workspace, seq);
            CREATE INDEX IF NOT EXISTS idx_doc_entries_workspace_branch ON doc_entries(workspace, branch);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_doc_entries_event_dedup ON doc_entries(workspace, branch, doc, source_event_id) WHERE source_event_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_graph_node_versions_seq ON graph_node_versions(workspace, branch, doc, seq);
            CREATE INDEX IF NOT EXISTS idx_graph_node_versions_key ON graph_node_versions(workspace, branch, doc, node_id, seq);
            CREATE INDEX IF NOT EXISTS idx_graph_edge_versions_seq ON graph_edge_versions(workspace, branch, doc, seq);
            CREATE INDEX IF NOT EXISTS idx_graph_edge_versions_key ON graph_edge_versions(workspace, branch, doc, from_id, rel, to_id, seq);
            CREATE INDEX IF NOT EXISTS idx_graph_conflicts_lookup ON graph_conflicts(workspace, into_branch, doc, status, created_at_ms);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_conflicts_dedup
              ON graph_conflicts(workspace, from_branch, into_branch, doc, kind, key, base_cutoff_seq, theirs_seq, ours_seq);
            CREATE INDEX IF NOT EXISTS idx_tasks_parent_plan ON tasks(workspace, parent_plan_id, id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_root_unique ON steps(workspace, task_id, ordinal) WHERE parent_step_id IS NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_child_unique ON steps(workspace, task_id, parent_step_id, ordinal) WHERE parent_step_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_steps_lookup ON steps(workspace, task_id, parent_step_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_steps_task_completed ON steps(workspace, task_id, completed, created_at_ms);
            CREATE INDEX IF NOT EXISTS idx_step_notes_step_seq ON step_notes(workspace, task_id, step_id, seq);
            CREATE INDEX IF NOT EXISTS idx_task_items_entity ON task_items(workspace, entity_kind, entity_id, field);
            CREATE INDEX IF NOT EXISTS idx_task_nodes_parent ON task_nodes(workspace, task_id, parent_step_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_ops_history_task ON ops_history(workspace, task_id, seq);
            "#,
        )?;
        add_column_if_missing(&self.conn, "plans", "description", "TEXT")?;
        add_column_if_missing(&self.conn, "plans", "context", "TEXT")?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "status",
            "TEXT NOT NULL DEFAULT 'TODO'",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "status_manual",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "priority",
            "TEXT NOT NULL DEFAULT 'MEDIUM'",
        )?;
        add_column_if_missing(&self.conn, "plans", "plan_doc", "TEXT")?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "plan_current",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "criteria_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "tests_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "criteria_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "tests_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "security_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "perf_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "plans",
            "docs_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;

        add_column_if_missing(
            &self.conn,
            "tasks",
            "status",
            "TEXT NOT NULL DEFAULT 'TODO'",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "status_manual",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "priority",
            "TEXT NOT NULL DEFAULT 'MEDIUM'",
        )?;
        add_column_if_missing(&self.conn, "tasks", "blocked", "INTEGER NOT NULL DEFAULT 0")?;
        add_column_if_missing(&self.conn, "tasks", "assignee", "TEXT")?;
        add_column_if_missing(&self.conn, "tasks", "domain", "TEXT")?;
        add_column_if_missing(&self.conn, "tasks", "phase", "TEXT")?;
        add_column_if_missing(&self.conn, "tasks", "component", "TEXT")?;
        add_column_if_missing(&self.conn, "tasks", "context", "TEXT")?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "criteria_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "tests_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "criteria_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "tests_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "security_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "perf_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "tasks",
            "docs_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;

        add_column_if_missing(&self.conn, "steps", "completed_at_ms", "INTEGER")?;
        add_column_if_missing(&self.conn, "steps", "started_at_ms", "INTEGER")?;
        add_column_if_missing(
            &self.conn,
            "steps",
            "criteria_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "steps",
            "tests_auto_confirmed",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        add_column_if_missing(
            &self.conn,
            "steps",
            "security_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "steps",
            "perf_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &self.conn,
            "steps",
            "docs_confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(&self.conn, "steps", "blocked", "INTEGER NOT NULL DEFAULT 0")?;
        add_column_if_missing(&self.conn, "steps", "block_reason", "TEXT")?;
        add_column_if_missing(&self.conn, "steps", "verification_outcome", "TEXT")?;
        self.conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES (?1, ?2)",
            params!["schema_version", "v0"],
        )?;
        Ok(())
    }

    pub fn create(
        &mut self,
        workspace: &WorkspaceId,
        kind: TaskKind,
        title: String,
        parent_plan_id: Option<String>,
        description: Option<String>,
        contract: Option<String>,
        contract_json: Option<String>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(String, i64, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let id = match kind {
            TaskKind::Plan => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "plan_seq")?;
                format!("PLAN-{:03}", seq)
            }
            TaskKind::Task => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "task_seq")?;
                format!("TASK-{:03}", seq)
            }
        };

        match kind {
            TaskKind::Plan => {
                tx.execute(
                    r#"
                    INSERT INTO plans(workspace,id,revision,title,contract,contract_json,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        title,
                        contract,
                        contract_json,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
            TaskKind::Task => {
                let parent_plan_id = parent_plan_id
                    .ok_or(StoreError::InvalidInput("parent is required for kind=task"))?;
                tx.execute(
                    r#"
                    INSERT INTO tasks(workspace,id,revision,parent_plan_id,title,description,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        parent_plan_id,
                        title,
                        description,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
        }

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            match kind {
                TaskKind::Plan => Some(id.clone()),
                TaskKind::Task => Some(id.clone()),
            },
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, &id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if matches!(kind, TaskKind::Task) {
            let touched = Self::project_task_graph_task_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &id,
                &title,
                now_ms,
            )?;
            if touched {
                touch_document_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.graph_doc,
                    now_ms,
                )?;
            }
        }

        tx.commit()?;
        Ok((id, 0i64, event))
    }

    pub fn edit_plan(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        title: Option<String>,
        description: Option<Option<String>>,
        context: Option<Option<String>>,
        priority: Option<String>,
        tags: Option<Vec<String>>,
        depends_on: Option<Vec<String>>,
        contract: Option<Option<String>>,
        contract_json: Option<Option<String>>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        if title.is_none()
            && description.is_none()
            && context.is_none()
            && priority.is_none()
            && tags.is_none()
            && depends_on.is_none()
            && contract.is_none()
            && contract_json.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, contract, contract_json, description, context, priority
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            revision,
            current_title,
            current_contract,
            current_contract_json,
            current_description,
            current_context,
            current_priority,
        )) = row
        else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        let new_revision = revision + 1;
        let new_title = title.unwrap_or(current_title);
        let new_contract = contract.unwrap_or(current_contract);
        let new_contract_json = contract_json.unwrap_or(current_contract_json);
        let new_description = description.unwrap_or(current_description);
        let new_context = context.unwrap_or(current_context);
        let new_priority = priority.unwrap_or(current_priority);

        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3,
                title = ?4,
                contract = ?5,
                contract_json = ?6,
                description = ?7,
                context = ?8,
                priority = ?9,
                updated_at_ms = ?10
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                id,
                new_revision,
                new_title,
                new_contract,
                new_contract_json,
                new_description,
                new_context,
                new_priority,
                now_ms
            ],
        )?;
        if let Some(items) = tags {
            task_items_replace_tx(&tx, workspace.as_str(), "plan", id, "tags", &items)?;
        }
        if let Some(items) = depends_on {
            task_items_replace_tx(&tx, workspace.as_str(), "plan", id, "depends_on", &items)?;
        }

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Plan, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn edit_task(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        title: Option<String>,
        description: Option<Option<String>>,
        context: Option<Option<String>>,
        priority: Option<String>,
        domain: Option<Option<String>>,
        phase: Option<Option<String>>,
        component: Option<Option<String>>,
        assignee: Option<Option<String>>,
        tags: Option<Vec<String>>,
        depends_on: Option<Vec<String>>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        if title.is_none()
            && description.is_none()
            && context.is_none()
            && priority.is_none()
            && domain.is_none()
            && phase.is_none()
            && component.is_none()
            && assignee.is_none()
            && tags.is_none()
            && depends_on.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, description, context, priority, domain, phase, component, assignee
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            revision,
            current_title,
            current_description,
            current_context,
            current_priority,
            current_domain,
            current_phase,
            current_component,
            current_assignee,
        )) = row
        else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        let new_revision = revision + 1;
        let new_title = title.unwrap_or(current_title);
        let new_description = description.unwrap_or(current_description);
        let new_context = context.unwrap_or(current_context);
        let new_priority = priority.unwrap_or(current_priority);
        let new_domain = domain.unwrap_or(current_domain);
        let new_phase = phase.unwrap_or(current_phase);
        let new_component = component.unwrap_or(current_component);
        let new_assignee = assignee.unwrap_or(current_assignee);

        tx.execute(
            r#"
            UPDATE tasks
            SET revision = ?3,
                title = ?4,
                description = ?5,
                context = ?6,
                priority = ?7,
                domain = ?8,
                phase = ?9,
                component = ?10,
                assignee = ?11,
                updated_at_ms = ?12
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                id,
                new_revision,
                new_title,
                new_description,
                new_context,
                new_priority,
                new_domain,
                new_phase,
                new_component,
                new_assignee,
                now_ms
            ],
        )?;
        if let Some(items) = tags {
            task_items_replace_tx(&tx, workspace.as_str(), "task", id, "tags", &items)?;
        }
        if let Some(items) = depends_on {
            task_items_replace_tx(&tx, workspace.as_str(), "task", id, "depends_on", &items)?;
        }

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let touched = Self::project_task_graph_task_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            id,
            &new_title,
            now_ms,
        )?;
        if touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn set_plan_status(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        status: &str,
        status_manual: bool,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                "SELECT revision, status FROM plans WHERE workspace = ?1 AND id = ?2",
                params![workspace.as_str(), id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((revision, _current_status)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        let new_revision = revision + 1;
        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3, status = ?4, status_manual = ?5, updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                id,
                new_revision,
                status,
                if status_manual { 1i64 } else { 0i64 },
                now_ms
            ],
        )?;

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;
        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Plan, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn set_task_status(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        status: &str,
        status_manual: bool,
        require_steps_completed: bool,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                "SELECT revision, status FROM tasks WHERE workspace = ?1 AND id = ?2",
                params![workspace.as_str(), id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((revision, _current_status)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        if require_steps_completed {
            let open_steps: i64 = tx.query_row(
                "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0",
                params![workspace.as_str(), id],
                |row| row.get(0),
            )?;
            if open_steps > 0 {
                return Err(StoreError::InvalidInput("steps not completed"));
            }
        }

        let new_revision = revision + 1;
        tx.execute(
            r#"
            UPDATE tasks
            SET revision = ?3, status = ?4, status_manual = ?5, updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                id,
                new_revision,
                status,
                if status_manual { 1i64 } else { 0i64 },
                now_ms
            ],
        )?;

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;
        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn get_plan(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<PlanRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, title, contract, contract_json, description, context,
                       status, status_manual, priority, plan_doc, plan_current,
                       criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       created_at_ms, updated_at_ms
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(PlanRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        title: row.get(2)?,
                        contract: row.get(3)?,
                        contract_json: row.get(4)?,
                        description: row.get(5)?,
                        context: row.get(6)?,
                        status: row.get(7)?,
                        status_manual: row.get::<_, i64>(8)? != 0,
                        priority: row.get(9)?,
                        plan_doc: row.get(10)?,
                        plan_current: row.get(11)?,
                        criteria_confirmed: row.get::<_, i64>(12)? != 0,
                        tests_confirmed: row.get::<_, i64>(13)? != 0,
                        criteria_auto_confirmed: row.get::<_, i64>(14)? != 0,
                        tests_auto_confirmed: row.get::<_, i64>(15)? != 0,
                        security_confirmed: row.get::<_, i64>(16)? != 0,
                        perf_confirmed: row.get::<_, i64>(17)? != 0,
                        docs_confirmed: row.get::<_, i64>(18)? != 0,
                        created_at_ms: row.get(19)?,
                        updated_at_ms: row.get(20)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn plan_checklist_get(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
    ) -> Result<PlanChecklist, StoreError> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT plan_doc, plan_current
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), plan_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((plan_doc, plan_current)) = row else {
            return Err(StoreError::UnknownId);
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT text
            FROM plan_checklist
            WHERE workspace = ?1 AND plan_id = ?2
            ORDER BY ordinal ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), plan_id], |row| {
            row.get::<_, String>(0)
        })?;
        let steps = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(PlanChecklist {
            doc: plan_doc,
            current: plan_current,
            steps,
        })
    }

    pub fn plan_checklist_update(
        &mut self,
        workspace: &WorkspaceId,
        plan_id: &str,
        expected_revision: Option<i64>,
        steps: Option<Vec<String>>,
        current: Option<i64>,
        doc: Option<String>,
        advance: bool,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, PlanChecklist, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, plan_doc, plan_current
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), plan_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((revision, current_doc, current_current)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        if let Some(items) = steps.as_ref() {
            tx.execute(
                "DELETE FROM plan_checklist WHERE workspace = ?1 AND plan_id = ?2",
                params![workspace.as_str(), plan_id],
            )?;
            for (ordinal, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO plan_checklist(workspace, plan_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), plan_id, ordinal as i64, text],
                )?;
            }
        }

        let mut next_current = current.unwrap_or(current_current);
        if advance {
            next_current = next_current.saturating_add(1);
        }

        let next_doc = doc.or(current_doc);
        let new_revision = revision + 1;
        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3,
                plan_doc = ?4,
                plan_current = ?5,
                updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                plan_id,
                new_revision,
                next_doc,
                next_current,
                now_ms
            ],
        )?;

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(plan_id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, plan_id, TaskKind::Plan, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;

        let checklist = self.plan_checklist_get(workspace, plan_id)?;
        Ok((new_revision, checklist, event))
    }

    pub fn get_task(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<TaskRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, parent_plan_id, title, description,
                       status, status_manual, priority, blocked,
                       assignee, domain, phase, component, context,
                       criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       created_at_ms, updated_at_ms
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(TaskRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        parent_plan_id: row.get(2)?,
                        title: row.get(3)?,
                        description: row.get(4)?,
                        status: row.get(5)?,
                        status_manual: row.get::<_, i64>(6)? != 0,
                        priority: row.get(7)?,
                        blocked: row.get::<_, i64>(8)? != 0,
                        assignee: row.get(9)?,
                        domain: row.get(10)?,
                        phase: row.get(11)?,
                        component: row.get(12)?,
                        context: row.get(13)?,
                        criteria_confirmed: row.get::<_, i64>(14)? != 0,
                        tests_confirmed: row.get::<_, i64>(15)? != 0,
                        criteria_auto_confirmed: row.get::<_, i64>(16)? != 0,
                        tests_auto_confirmed: row.get::<_, i64>(17)? != 0,
                        security_confirmed: row.get::<_, i64>(18)? != 0,
                        perf_confirmed: row.get::<_, i64>(19)? != 0,
                        docs_confirmed: row.get::<_, i64>(20)? != 0,
                        created_at_ms: row.get(21)?,
                        updated_at_ms: row.get(22)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn focus_set(&mut self, workspace: &WorkspaceId, focus_id: &str) -> Result<(), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        tx.execute(
            r#"
            INSERT INTO focus(workspace, focus_id, updated_at_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace) DO UPDATE SET focus_id=excluded.focus_id, updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace.as_str(), focus_id, now_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn focus_get(&self, workspace: &WorkspaceId) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT focus_id FROM focus WHERE workspace = ?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn focus_clear(&mut self, workspace: &WorkspaceId) -> Result<bool, StoreError> {
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM focus WHERE workspace = ?1",
            params![workspace.as_str()],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn ensure_reasoning_ref(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        kind: TaskKind,
    ) -> Result<ReasoningRefRow, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let exists = match kind {
            TaskKind::Plan => tx
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
            TaskKind::Task => tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
        };

        if !exists {
            return Err(StoreError::UnknownId);
        }

        if let Some(row) = tx
            .query_row(
                r#"
                SELECT branch, notes_doc, graph_doc, trace_doc
                FROM reasoning_refs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(ReasoningRefRow {
                        branch: row.get(0)?,
                        notes_doc: row.get(1)?,
                        graph_doc: row.get(2)?,
                        trace_doc: row.get(3)?,
                    })
                },
            )
            .optional()?
        {
            tx.commit()?;
            return Ok(row);
        }

        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let reference = ReasoningRef::for_entity(kind, id);
        tx.execute(
            r#"
            INSERT INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                workspace.as_str(),
                id,
                kind.as_str(),
                reference.branch,
                reference.notes_doc,
                reference.graph_doc,
                reference.trace_doc,
                now_ms
            ],
        )?;

        let row = ReasoningRefRow {
            branch: reference.branch,
            notes_doc: reference.notes_doc,
            graph_doc: reference.graph_doc,
            trace_doc: reference.trace_doc,
        };

        tx.commit()?;
        Ok(row)
    }

    pub fn doc_append_note(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        title: Option<String>,
        format: Option<String>,
        meta_json: Option<String>,
        content: String,
    ) -> Result<DocEntryRow, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if content.trim().is_empty() {
            return Err(StoreError::InvalidInput("content must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;

        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                workspace.as_str(),
                branch,
                doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                title.as_deref(),
                format.as_deref(),
                meta_json.as_deref(),
                &content
            ],
        )?;
        let seq = tx.last_insert_rowid();
        touch_document_tx(&tx, workspace.as_str(), branch, doc, now_ms)?;

        tx.commit()?;
        Ok(DocEntryRow {
            seq,
            ts_ms: now_ms,
            branch: branch.to_string(),
            doc: doc.to_string(),
            kind: DocEntryKind::Note,
            title,
            format,
            meta_json,
            content: Some(content),
            source_event_id: None,
            event_type: None,
            task_id: None,
            path: None,
            payload_json: None,
        })
    }

    pub fn doc_append_trace(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        title: Option<String>,
        format: Option<String>,
        meta_json: Option<String>,
        content: String,
    ) -> Result<DocEntryRow, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if content.trim().is_empty() {
            return Err(StoreError::InvalidInput("content must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            doc,
            DocumentKind::Trace.as_str(),
            now_ms,
        )?;

        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                workspace.as_str(),
                branch,
                doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                title.as_deref(),
                format.as_deref(),
                meta_json.as_deref(),
                &content
            ],
        )?;
        let seq = tx.last_insert_rowid();
        touch_document_tx(&tx, workspace.as_str(), branch, doc, now_ms)?;

        tx.commit()?;
        Ok(DocEntryRow {
            seq,
            ts_ms: now_ms,
            branch: branch.to_string(),
            doc: doc.to_string(),
            kind: DocEntryKind::Note,
            title,
            format,
            meta_json,
            content: Some(content),
            source_event_id: None,
            event_type: None,
            task_id: None,
            path: None,
            payload_json: None,
        })
    }

    pub fn doc_list(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<Vec<DocumentRow>, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let docs = {
            let mut stmt = tx.prepare(
                "SELECT doc, kind, created_at_ms, updated_at_ms \
                 FROM documents WHERE workspace=?1 AND branch=?2 \
                 ORDER BY updated_at_ms DESC, doc ASC",
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), branch])?;
            let mut docs = Vec::new();

            while let Some(row) = rows.next()? {
                let kind: String = row.get(1)?;
                let kind = match kind.as_str() {
                    "notes" => DocumentKind::Notes,
                    "trace" => DocumentKind::Trace,
                    "graph" => DocumentKind::Graph,
                    _ => DocumentKind::Notes,
                };
                docs.push(DocumentRow {
                    branch: branch.to_string(),
                    doc: row.get(0)?,
                    kind,
                    created_at_ms: row.get(2)?,
                    updated_at_ms: row.get(3)?,
                });
            }

            docs
        };

        tx.commit()?;
        Ok(docs)
    }

    pub fn doc_show_tail(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        let mut entries_desc = Vec::new();
        {
            let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

            let mut sql = String::from(
                "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
                 FROM doc_entries \
                 WHERE workspace=? AND doc=? AND seq < ? AND (",
            );
            let mut params: Vec<SqlValue> = Vec::new();
            params.push(SqlValue::Text(workspace.as_str().to_string()));
            params.push(SqlValue::Text(doc.to_string()));
            params.push(SqlValue::Integer(before_seq));

            for (index, source) in sources.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("(branch=?");
                params.push(SqlValue::Text(source.branch.clone()));
                if let Some(cutoff) = source.cutoff_seq {
                    sql.push_str(" AND seq <= ?");
                    params.push(SqlValue::Integer(cutoff));
                }
                sql.push(')');
            }

            sql.push_str(") ORDER BY seq DESC LIMIT ?");
            params.push(SqlValue::Integer(limit + 1));

            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params))?;

            while let Some(row) = rows.next()? {
                let kind_str: String = row.get(3)?;
                let kind = match kind_str.as_str() {
                    "note" => DocEntryKind::Note,
                    "event" => DocEntryKind::Event,
                    _ => DocEntryKind::Event,
                };
                entries_desc.push(DocEntryRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    branch: row.get(2)?,
                    doc: doc.to_string(),
                    kind,
                    title: row.get(4)?,
                    format: row.get(5)?,
                    meta_json: row.get(6)?,
                    content: row.get(7)?,
                    source_event_id: row.get(8)?,
                    event_type: row.get(9)?,
                    task_id: row.get(10)?,
                    path: row.get(11)?,
                    payload_json: row.get(12)?,
                });
            }
        }

        let has_more = entries_desc.len() as i64 > limit;
        if has_more {
            entries_desc.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            entries_desc.last().map(|e| e.seq)
        } else {
            None
        };

        entries_desc.reverse();
        tx.commit()?;

        Ok(DocSlice {
            entries: entries_desc,
            next_cursor,
            has_more,
        })
    }

    pub fn doc_diff_tail(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        to_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if to_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("to_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), to_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let slice = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            from_branch,
            to_branch,
            doc,
            before_seq,
            limit,
        )?;
        tx.commit()?;
        Ok(slice)
    }

    pub fn doc_merge_notes(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        into_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
        dry_run: bool,
    ) -> Result<MergeNotesResult, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), into_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        if !dry_run {
            ensure_workspace_tx(&tx, workspace, now_ms)?;
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                into_branch,
                doc,
                DocumentKind::Notes.as_str(),
                now_ms,
            )?;
        }

        // Merge candidates are entries present in sourceView(from_branch) but not in destView(into_branch).
        let diff = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            into_branch,
            from_branch,
            doc,
            before_seq,
            limit,
        )?;

        let mut merged = 0usize;
        let mut skipped = 0usize;
        let mut touched = false;

        for entry in diff.entries.iter() {
            if entry.kind != DocEntryKind::Note {
                skipped += 1;
                continue;
            }

            let Some(content) = entry.content.as_deref() else {
                skipped += 1;
                continue;
            };

            let merge_key = format!("merge:{from_branch}:{}", entry.seq);
            if dry_run {
                let exists = tx
                    .query_row(
                        "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4 LIMIT 1",
                        params![workspace.as_str(), into_branch, doc, &merge_key],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if exists {
                    skipped += 1;
                } else {
                    merged += 1;
                }
                continue;
            }

            let meta_json = merge_meta_json(
                entry.meta_json.as_deref(),
                from_branch,
                entry.seq,
                entry.ts_ms,
            );

            let inserted = tx.execute(
                r#"
                INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content, source_event_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    workspace.as_str(),
                    into_branch,
                    doc,
                    now_ms,
                    DocEntryKind::Note.as_str(),
                    entry.title.as_deref(),
                    entry.format.as_deref(),
                    &meta_json,
                    content,
                    &merge_key
                ],
            )?;

            if inserted > 0 {
                merged += 1;
                touched = true;
            } else {
                skipped += 1;
            }
        }

        if touched {
            touch_document_tx(&tx, workspace.as_str(), into_branch, doc, now_ms)?;
        }

        tx.commit()?;
        Ok(MergeNotesResult {
            merged,
            skipped,
            count: diff.entries.len(),
            next_cursor: diff.next_cursor,
            has_more: diff.has_more,
        })
    }

    pub fn doc_head_seq_for_branch_doc(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
    ) -> Result<Option<i64>, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }
        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
        let head_seq = doc_head_seq_for_sources_tx(&tx, workspace.as_str(), doc, &sources)?;
        tx.commit()?;
        Ok(head_seq)
    }

    pub fn doc_entry_visible(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        seq: i64,
    ) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        let visible = doc_entry_visible_tx(&tx, workspace.as_str(), branch, doc, seq)?;
        tx.commit()?;
        Ok(visible)
    }

    pub fn vcs_ref_get(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        doc: &str,
    ) -> Result<Option<VcsRefRow>, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT branch, seq, updated_at_ms FROM vcs_refs WHERE workspace=?1 AND ref=?2 AND doc=?3",
                params![workspace.as_str(), reference, doc],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()?;
        tx.commit()?;
        Ok(row.map(|(branch, seq, updated_at_ms)| VcsRefRow {
            reference: reference.to_string(),
            branch,
            doc: doc.to_string(),
            seq,
            updated_at_ms,
        }))
    }

    pub fn vcs_ref_set(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        branch: &str,
        doc: &str,
        seq: i64,
        message: Option<String>,
    ) -> Result<VcsRefUpdate, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if seq <= 0 {
            return Err(StoreError::InvalidInput("seq must be positive"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let existing = tx
            .query_row(
                "SELECT branch, seq, updated_at_ms FROM vcs_refs WHERE workspace=?1 AND ref=?2 AND doc=?3",
                params![workspace.as_str(), reference, doc],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()?;

        let old_seq = existing.as_ref().map(|(_, seq, _)| *seq);
        let existing_branch = existing.as_ref().map(|(branch, _, _)| branch.as_str());
        let needs_update = existing_branch != Some(branch) || old_seq != Some(seq);

        if needs_update {
            tx.execute(
                r#"
                INSERT INTO vcs_refs(workspace, ref, doc, branch, seq, updated_at_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(workspace, ref, doc) DO UPDATE SET
                  branch=excluded.branch,
                  seq=excluded.seq,
                  updated_at_ms=excluded.updated_at_ms
                "#,
                params![
                    workspace.as_str(),
                    reference,
                    doc,
                    branch,
                    seq,
                    now_ms
                ],
            )?;

            tx.execute(
                r#"
                INSERT INTO vcs_reflog(workspace, ref, doc, branch, old_seq, new_seq, message, ts_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    workspace.as_str(),
                    reference,
                    doc,
                    branch,
                    old_seq,
                    seq,
                    message.as_deref(),
                    now_ms
                ],
            )?;
        }

        tx.commit()?;
        Ok(VcsRefUpdate {
            reference: VcsRefRow {
                reference: reference.to_string(),
                branch: branch.to_string(),
                doc: doc.to_string(),
                seq,
                updated_at_ms: now_ms,
            },
            old_seq,
        })
    }

    pub fn vcs_reflog_list(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        doc: &str,
        limit: usize,
    ) -> Result<Vec<VcsReflogRow>, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;
        let out = {
            let mut stmt = tx.prepare(
                "SELECT branch, old_seq, new_seq, message, ts_ms \
                 FROM vcs_reflog WHERE workspace=?1 AND ref=?2 AND doc=?3 \
                 ORDER BY ts_ms DESC, new_seq DESC LIMIT ?4",
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), reference, doc, limit])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(VcsReflogRow {
                    reference: reference.to_string(),
                    branch: row.get(0)?,
                    doc: doc.to_string(),
                    old_seq: row.get(1)?,
                    new_seq: row.get(2)?,
                    message: row.get(3)?,
                    ts_ms: row.get(4)?,
                });
            }
            out
        };
        tx.commit()?;
        Ok(out)
    }

    pub fn vcs_tag_get(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<Option<VcsTagRow>, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        let tx = self.conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT branch, doc, seq, created_at_ms FROM vcs_tags WHERE workspace=?1 AND name=?2",
                params![workspace.as_str(), name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
            )
            .optional()?;
        tx.commit()?;
        Ok(row.map(|(branch, doc, seq, created_at_ms)| VcsTagRow {
            name: name.to_string(),
            branch,
            doc,
            seq,
            created_at_ms,
        }))
    }

    pub fn vcs_tag_create(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
        branch: &str,
        doc: &str,
        seq: i64,
        force: bool,
    ) -> Result<VcsTagRow, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if seq <= 0 {
            return Err(StoreError::InvalidInput("seq must be positive"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }
        if !doc_entry_visible_tx(&tx, workspace.as_str(), branch, doc, seq)? {
            return Err(StoreError::InvalidInput("commit not visible for branch"));
        }

        if !force {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM vcs_tags WHERE workspace=?1 AND name=?2 LIMIT 1",
                    params![workspace.as_str(), name],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                return Err(StoreError::InvalidInput("tag already exists"));
            }
        }

        tx.execute(
            r#"
            INSERT INTO vcs_tags(workspace, name, doc, branch, seq, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(workspace, name) DO UPDATE SET
              doc=excluded.doc,
              branch=excluded.branch,
              seq=excluded.seq,
              created_at_ms=excluded.created_at_ms
            "#,
            params![workspace.as_str(), name, doc, branch, seq, now_ms],
        )?;
        tx.commit()?;
        Ok(VcsTagRow {
            name: name.to_string(),
            branch: branch.to_string(),
            doc: doc.to_string(),
            seq,
            created_at_ms: now_ms,
        })
    }

    pub fn vcs_tag_list(&mut self, workspace: &WorkspaceId) -> Result<Vec<VcsTagRow>, StoreError> {
        let tx = self.conn.transaction()?;
        let tags = {
            let mut stmt = tx.prepare(
                "SELECT name, branch, doc, seq, created_at_ms FROM vcs_tags \
                 WHERE workspace=?1 ORDER BY created_at_ms DESC, name ASC",
            )?;
            let mut rows = stmt.query(params![workspace.as_str()])?;
            let mut tags = Vec::new();
            while let Some(row) = rows.next()? {
                tags.push(VcsTagRow {
                    name: row.get(0)?,
                    branch: row.get(1)?,
                    doc: row.get(2)?,
                    seq: row.get(3)?,
                    created_at_ms: row.get(4)?,
                });
            }
            tags
        };
        tx.commit()?;
        Ok(tags)
    }

    pub fn vcs_tag_delete(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<bool, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM vcs_tags WHERE workspace=?1 AND name=?2",
            params![workspace.as_str(), name],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn graph_apply_ops(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        ops: Vec<GraphOp>,
    ) -> Result<GraphApplyResult, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if ops.is_empty() {
            return Err(StoreError::InvalidInput("ops must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;

        let mut nodes_upserted = 0usize;
        let mut nodes_deleted = 0usize;
        let mut edges_upserted = 0usize;
        let mut edges_deleted = 0usize;
        let mut last_seq = 0i64;

        for op in ops {
            let (content, seq_opt) =
                insert_graph_doc_entry_tx(&tx, workspace.as_str(), branch, doc, now_ms, &op, None)?;
            let Some(seq) = seq_opt else {
                // No dedup key was provided, so this should be unreachable.
                return Err(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows));
            };
            last_seq = seq;

            match op {
                GraphOp::NodeUpsert(upsert) => {
                    validate_graph_node_id(&upsert.id)?;
                    validate_graph_type(&upsert.node_type)?;
                    let tags = normalize_tags(&upsert.tags)?;
                    insert_graph_node_version_tx(
                        &tx,
                        workspace.as_str(),
                        branch,
                        doc,
                        seq,
                        now_ms,
                        &upsert.id,
                        Some(&upsert.node_type),
                        upsert.title.as_deref(),
                        upsert.text.as_deref(),
                        &tags,
                        upsert.status.as_deref(),
                        upsert.meta_json.as_deref(),
                        false,
                    )?;
                    nodes_upserted += 1;
                }
                GraphOp::NodeDelete { id } => {
                    validate_graph_node_id(&id)?;
                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let Some(existing) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &id)?
                    else {
                        return Err(StoreError::InvalidInput("node not found"));
                    };
                    if existing.deleted {
                        return Err(StoreError::InvalidInput("node already deleted"));
                    }

                    insert_graph_node_version_tx(
                        &tx,
                        workspace.as_str(),
                        branch,
                        doc,
                        seq,
                        now_ms,
                        &id,
                        Some(existing.node_type.as_str()),
                        existing.title.as_deref(),
                        existing.text.as_deref(),
                        &existing.tags,
                        existing.status.as_deref(),
                        existing.meta_json.as_deref(),
                        true,
                    )?;
                    nodes_deleted += 1;

                    // Cascade-delete edges connected to this node in the current effective view.
                    let edge_keys =
                        graph_edge_keys_for_node_tx(&tx, workspace.as_str(), &sources, doc, &id)?;
                    for key in edge_keys {
                        insert_graph_edge_version_tx(
                            &tx,
                            workspace.as_str(),
                            branch,
                            doc,
                            seq,
                            now_ms,
                            &key.from,
                            &key.rel,
                            &key.to,
                            None,
                            true,
                        )?;
                        edges_deleted += 1;
                    }
                }
                GraphOp::EdgeUpsert(upsert) => {
                    validate_graph_node_id(&upsert.from)?;
                    validate_graph_node_id(&upsert.to)?;
                    validate_graph_rel(&upsert.rel)?;

                    // Require endpoints to exist in the effective view (avoid dangling edges).
                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let Some(from_node) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &upsert.from)?
                    else {
                        return Err(StoreError::InvalidInput("edge.from node not found"));
                    };
                    if from_node.deleted {
                        return Err(StoreError::InvalidInput("edge.from node is deleted"));
                    }
                    let Some(to_node) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &upsert.to)?
                    else {
                        return Err(StoreError::InvalidInput("edge.to node not found"));
                    };
                    if to_node.deleted {
                        return Err(StoreError::InvalidInput("edge.to node is deleted"));
                    }

                    insert_graph_edge_version_tx(
                        &tx,
                        workspace.as_str(),
                        branch,
                        doc,
                        seq,
                        now_ms,
                        &upsert.from,
                        &upsert.rel,
                        &upsert.to,
                        upsert.meta_json.as_deref(),
                        false,
                    )?;
                    edges_upserted += 1;
                }
                GraphOp::EdgeDelete { from, rel, to } => {
                    validate_graph_node_id(&from)?;
                    validate_graph_node_id(&to)?;
                    validate_graph_rel(&rel)?;

                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let key = GraphEdgeKey {
                        from: from.clone(),
                        rel: rel.clone(),
                        to: to.clone(),
                    };
                    let Some(existing) =
                        graph_edge_get_tx(&tx, workspace.as_str(), &sources, doc, &key)?
                    else {
                        return Err(StoreError::InvalidInput("edge not found"));
                    };
                    if existing.deleted {
                        return Err(StoreError::InvalidInput("edge already deleted"));
                    }

                    insert_graph_edge_version_tx(
                        &tx,
                        workspace.as_str(),
                        branch,
                        doc,
                        seq,
                        now_ms,
                        &from,
                        &rel,
                        &to,
                        existing.meta_json.as_deref(),
                        true,
                    )?;
                    edges_deleted += 1;
                }
            }

            let _ = content;
        }

        touch_document_tx(&tx, workspace.as_str(), branch, doc, now_ms)?;
        tx.commit()?;

        Ok(GraphApplyResult {
            nodes_upserted,
            nodes_deleted,
            edges_upserted,
            edges_deleted,
            last_seq,
            last_ts_ms: now_ms,
        })
    }

    fn project_task_graph_task_node_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        task_id: &str,
        title: &str,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let node_id = task_graph_node_id(task_id);
        let meta_json = build_task_graph_meta_json(task_id);
        let source_event_id = format!("task_graph:{}:node:{node_id}", event.event_id());
        graph_upsert_node_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            now_ms,
            &node_id,
            "task",
            Some(title),
            None,
            Some(meta_json.as_str()),
            &source_event_id,
        )
    }

    fn project_task_graph_step_node_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        task_id: &str,
        step: &StepRef,
        title: &str,
        completed: bool,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let node_id = step_graph_node_id(&step.step_id);
        let meta_json = build_step_graph_meta_json(task_id, step);
        let status = if completed {
            Some("done")
        } else {
            Some("open")
        };
        let source_event_id = format!("task_graph:{}:node:{node_id}", event.event_id());
        graph_upsert_node_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            now_ms,
            &node_id,
            "step",
            Some(title),
            status,
            Some(meta_json.as_str()),
            &source_event_id,
        )
    }

    fn project_task_graph_contains_edge_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        from: &str,
        to: &str,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let source_event_id = format!("task_graph:{}:edge:{from}:contains:{to}", event.event_id());
        graph_upsert_edge_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            now_ms,
            from,
            "contains",
            to,
            None,
            &source_event_id,
        )
    }

    pub fn think_card_commit(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        trace_doc: &str,
        graph_doc: &str,
        card: ThinkCardInput,
        supports: &[String],
        blocks: &[String],
    ) -> Result<ThinkCardCommitResult, StoreError> {
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        let trace_doc = trace_doc.trim();
        if trace_doc.is_empty() {
            return Err(StoreError::InvalidInput("trace_doc must not be empty"));
        }
        let graph_doc = graph_doc.trim();
        if graph_doc.is_empty() {
            return Err(StoreError::InvalidInput("graph_doc must not be empty"));
        }

        let card_id = card.card_id.trim();
        if card_id.is_empty() {
            return Err(StoreError::InvalidInput("card.id must not be empty"));
        }
        let card_type = card.card_type.trim();
        if card_type.is_empty() {
            return Err(StoreError::InvalidInput("card.type must not be empty"));
        }
        if !bm_core::think::is_supported_think_card_type(card_type) {
            return Err(StoreError::InvalidInput("unsupported card.type"));
        }
        validate_graph_node_id(card_id)?;
        validate_graph_type(card_type)?;

        let title_ok = card
            .title
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let text_ok = card
            .text
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if !title_ok && !text_ok {
            return Err(StoreError::InvalidInput(
                "card must have at least one of title or text",
            ));
        }
        let tags = normalize_tags(&card.tags)?;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        // 1) Trace: idempotent note entry keyed by card_id.
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            trace_doc,
            DocumentKind::Trace.as_str(),
            now_ms,
        )?;

        let trace_source_event_id = format!("think_card:{card_id}");
        let existing_payload: Option<Option<String>> = tx
            .query_row(
                r#"
                SELECT payload_json
                FROM doc_entries
                WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4
                LIMIT 1
                "#,
                params![
                    workspace.as_str(),
                    branch,
                    trace_doc,
                    trace_source_event_id.as_str()
                ],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;

        let mut inserted = false;
        match existing_payload {
            Some(Some(existing)) => {
                if existing != card.payload_json {
                    return Err(StoreError::InvalidInput(
                        "card_id already exists with a different payload",
                    ));
                }
            }
            Some(None) => {
                return Err(StoreError::InvalidInput(
                    "card_id already exists but stored payload is missing",
                ));
            }
            None => {
                let inserted_rows = tx.execute(
                    r#"
                    INSERT OR IGNORE INTO doc_entries(
                      workspace, branch, doc, ts_ms, kind, title, format, meta_json, content,
                      source_event_id, event_type, payload_json
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                    "#,
                    params![
                        workspace.as_str(),
                        branch,
                        trace_doc,
                        now_ms,
                        DocEntryKind::Note.as_str(),
                        card.title.as_deref(),
                        "think_card",
                        card.meta_json.as_deref(),
                        card.content.as_str(),
                        trace_source_event_id.as_str(),
                        "think_card",
                        card.payload_json.as_str(),
                    ],
                )?;
                inserted = inserted_rows > 0;
                if inserted {
                    touch_document_tx(&tx, workspace.as_str(), branch, trace_doc, now_ms)?;
                }
            }
        }

        // 2) Graph: idempotent semantic upserts for node + support/block edges.
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;

        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

        let mut nodes_upserted = 0usize;
        let mut edges_upserted = 0usize;
        let mut last_seq: Option<i64> = None;
        let mut touched_graph = false;

        let existing_node =
            graph_node_get_tx(&tx, workspace.as_str(), &sources, graph_doc, card_id)?;
        let candidate_node = GraphNodeRow {
            id: card_id.to_string(),
            node_type: card_type.to_string(),
            title: card.title.clone(),
            text: card.text.clone(),
            tags: tags.clone(),
            status: card.status.clone(),
            meta_json: card.meta_json.clone(),
            deleted: false,
            last_seq: 0,
            last_ts_ms: 0,
        };

        if !graph_node_semantic_eq(existing_node.as_ref(), Some(&candidate_node)) {
            let op = GraphOp::NodeUpsert(GraphNodeUpsert {
                id: candidate_node.id.clone(),
                node_type: candidate_node.node_type.clone(),
                title: candidate_node.title.clone(),
                text: candidate_node.text.clone(),
                tags: tags.clone(),
                status: candidate_node.status.clone(),
                meta_json: candidate_node.meta_json.clone(),
            });
            let dedup = format!("think_card:{card_id}:node");
            let (_payload, seq_opt) = insert_graph_doc_entry_tx(
                &tx,
                workspace.as_str(),
                branch,
                graph_doc,
                now_ms,
                &op,
                Some(&dedup),
            )?;
            let Some(seq) = seq_opt else {
                return Err(StoreError::InvalidInput(
                    "dedup prevented node write (card_id collision)",
                ));
            };
            insert_graph_node_version_tx(
                &tx,
                workspace.as_str(),
                branch,
                graph_doc,
                seq,
                now_ms,
                &candidate_node.id,
                Some(&candidate_node.node_type),
                candidate_node.title.as_deref(),
                candidate_node.text.as_deref(),
                &tags,
                candidate_node.status.as_deref(),
                candidate_node.meta_json.as_deref(),
                false,
            )?;
            nodes_upserted += 1;
            last_seq = Some(seq);
            touched_graph = true;
        }

        let mut upsert_edge = |rel: &str, to_id: &str| -> Result<(), StoreError> {
            validate_graph_rel(rel)?;
            validate_graph_node_id(to_id)?;
            let key = GraphEdgeKey {
                from: card_id.to_string(),
                rel: rel.to_string(),
                to: to_id.to_string(),
            };
            let existing = graph_edge_get_tx(&tx, workspace.as_str(), &sources, graph_doc, &key)?;
            let candidate = GraphEdgeRow {
                from: key.from.clone(),
                rel: key.rel.clone(),
                to: key.to.clone(),
                meta_json: None,
                deleted: false,
                last_seq: 0,
                last_ts_ms: 0,
            };
            if graph_edge_semantic_eq(existing.as_ref(), Some(&candidate)) {
                return Ok(());
            }

            let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
                from: key.from.clone(),
                rel: key.rel.clone(),
                to: key.to.clone(),
                meta_json: None,
            });
            let dedup = format!("think_card:{card_id}:edge:{rel}:{to_id}");
            let (_payload, seq_opt) = insert_graph_doc_entry_tx(
                &tx,
                workspace.as_str(),
                branch,
                graph_doc,
                now_ms,
                &op,
                Some(&dedup),
            )?;
            let Some(seq) = seq_opt else {
                return Err(StoreError::InvalidInput(
                    "dedup prevented edge write (card_id collision)",
                ));
            };
            insert_graph_edge_version_tx(
                &tx,
                workspace.as_str(),
                branch,
                graph_doc,
                seq,
                now_ms,
                &key.from,
                &key.rel,
                &key.to,
                None,
                false,
            )?;
            edges_upserted += 1;
            last_seq = Some(seq);
            touched_graph = true;
            Ok(())
        };

        for to_id in supports {
            upsert_edge("supports", to_id)?;
        }
        for to_id in blocks {
            upsert_edge("blocks", to_id)?;
        }

        if touched_graph {
            touch_document_tx(&tx, workspace.as_str(), branch, graph_doc, now_ms)?;
        }

        tx.commit()?;

        Ok(ThinkCardCommitResult {
            inserted,
            nodes_upserted,
            edges_upserted,
            last_seq,
        })
    }

    pub fn graph_query(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        request: GraphQueryRequest,
    ) -> Result<GraphQuerySlice, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let cursor = request.cursor.unwrap_or(i64::MAX);
        let limit = request.limit.clamp(1, 200) as i64;
        let edges_limit = request.edges_limit.clamp(0, 1000) as i64;
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

        let mut nodes = graph_nodes_query_tx(
            &tx,
            workspace.as_str(),
            &sources,
            doc,
            cursor,
            limit,
            &request,
        )?;

        let has_more = nodes.len() as i64 > limit;
        if has_more {
            nodes.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            nodes.last().map(|n| n.last_seq)
        } else {
            None
        };

        let mut edges = Vec::new();
        if request.include_edges && !nodes.is_empty() && edges_limit > 0 {
            let node_ids = nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
            edges = graph_edges_for_nodes_tx(
                &tx,
                workspace.as_str(),
                &sources,
                doc,
                &node_ids,
                edges_limit,
            )?;
        }

        tx.commit()?;
        Ok(GraphQuerySlice {
            nodes,
            edges,
            next_cursor,
            has_more,
        })
    }

    pub fn graph_validate(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        max_errors: usize,
    ) -> Result<GraphValidateResult, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let max_errors = max_errors.clamp(1, 500);
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
        let nodes = graph_nodes_all_tx(&tx, workspace.as_str(), &sources, doc, false)?;
        let edges = graph_edges_all_tx(&tx, workspace.as_str(), &sources, doc, false)?;

        use std::collections::HashSet;
        let mut node_set = HashSet::new();
        for node in nodes.iter() {
            if !node.deleted {
                node_set.insert(node.id.as_str());
            }
        }

        let mut errors = Vec::new();
        for edge in edges.iter() {
            if edge.deleted {
                continue;
            }
            if !node_set.contains(edge.from.as_str()) || !node_set.contains(edge.to.as_str()) {
                let key = format!("{}|{}|{}", edge.from, edge.rel, edge.to);
                errors.push(GraphValidateError {
                    code: "EDGE_ENDPOINT_MISSING",
                    message: "edge endpoint is missing or deleted".to_string(),
                    kind: "edge",
                    key,
                });
                if errors.len() >= max_errors {
                    break;
                }
            }
        }

        tx.commit()?;
        Ok(GraphValidateResult {
            ok: errors.is_empty(),
            nodes: nodes.into_iter().filter(|n| !n.deleted).count(),
            edges: edges.into_iter().filter(|e| !e.deleted).count(),
            errors,
        })
    }

    pub fn graph_diff(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        to_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<GraphDiffSlice, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if to_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("to_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let scan_limit = (limit * 5).clamp(limit, 1000);
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), to_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let from_sources = branch_sources_tx(&tx, workspace.as_str(), from_branch)?;
        let to_sources = branch_sources_tx(&tx, workspace.as_str(), to_branch)?;

        let candidates = graph_diff_candidates_tx(
            &tx,
            workspace.as_str(),
            &to_sources,
            doc,
            before_seq,
            scan_limit + 1,
        )?;

        let mut changes = Vec::new();
        let mut scanned = 0usize;

        let mut node_ids = Vec::new();
        let mut edge_keys = Vec::new();
        for candidate in candidates.iter().take(scan_limit as usize) {
            match candidate {
                GraphDiffCandidate::Node { to, .. } => node_ids.push(to.id.clone()),
                GraphDiffCandidate::Edge { key, .. } => edge_keys.push(key.clone()),
            }
        }

        let from_nodes =
            graph_nodes_get_map_tx(&tx, workspace.as_str(), &from_sources, doc, &node_ids, true)?;
        let from_edges = graph_edges_get_map_tx(
            &tx,
            workspace.as_str(),
            &from_sources,
            doc,
            &edge_keys,
            true,
        )?;

        for candidate in candidates.iter().take(scan_limit as usize) {
            if changes.len() as i64 >= limit {
                break;
            }
            scanned += 1;
            match candidate {
                GraphDiffCandidate::Node { to, .. } => {
                    let from = from_nodes.get(&to.id);
                    if !graph_node_semantic_eq(from, Some(to)) {
                        changes.push(GraphDiffChange::Node { to: to.clone() });
                    }
                }
                GraphDiffCandidate::Edge { key, to, .. } => {
                    let from = from_edges.get(key);
                    if !graph_edge_semantic_eq(from, Some(to)) {
                        changes.push(GraphDiffChange::Edge { to: to.clone() });
                    }
                }
            }
        }

        let has_more = candidates.len() > scanned;
        let next_cursor = if has_more {
            candidates
                .get(scanned.saturating_sub(1))
                .map(|c| c.last_seq())
        } else {
            None
        };

        tx.commit()?;
        Ok(GraphDiffSlice {
            changes,
            next_cursor,
            has_more,
        })
    }

    pub fn graph_merge_back(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        into_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
        dry_run: bool,
    ) -> Result<GraphMergeResult, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let scan_limit = (limit * 5).clamp(limit, 1000);
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), into_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let Some((base_branch, base_cutoff_seq)) =
            branch_base_info_tx(&tx, workspace.as_str(), from_branch)?
        else {
            return Err(StoreError::MergeNotSupported);
        };
        if base_branch != into_branch {
            return Err(StoreError::MergeNotSupported);
        }

        if !dry_run {
            ensure_workspace_tx(&tx, workspace, now_ms)?;
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                into_branch,
                doc,
                DocumentKind::Graph.as_str(),
                now_ms,
            )?;
        }

        let base_sources = base_sources_for_branch_tx(&tx, workspace.as_str(), from_branch)?;
        let into_sources = branch_sources_tx(&tx, workspace.as_str(), into_branch)?;

        let candidates = graph_merge_candidates_tx(
            &tx,
            workspace.as_str(),
            from_branch,
            doc,
            base_cutoff_seq,
            before_seq,
            scan_limit + 1,
        )?;

        let mut merged = 0usize;
        let mut skipped = 0usize;
        let mut conflicts_created = 0usize;
        let mut conflict_ids = Vec::new();
        let mut processed = 0usize;

        for candidate in candidates.iter().take(scan_limit as usize) {
            if merged as i64 + skipped as i64 + conflicts_created as i64 >= limit {
                break;
            }
            processed += 1;

            match candidate {
                GraphMergeCandidate::Node { theirs, .. } => {
                    let key = theirs.id.clone();
                    let base =
                        graph_node_get_tx(&tx, workspace.as_str(), &base_sources, doc, &key)?;
                    let ours =
                        graph_node_get_tx(&tx, workspace.as_str(), &into_sources, doc, &key)?;

                    if graph_node_semantic_eq(base.as_ref(), Some(theirs))
                        || graph_node_semantic_eq(ours.as_ref(), Some(theirs))
                    {
                        skipped += 1;
                        continue;
                    }
                    if graph_node_semantic_eq(base.as_ref(), ours.as_ref()) {
                        if dry_run {
                            merged += 1;
                            continue;
                        }
                        let merge_key =
                            format!("graph_merge:{from_branch}:{}:node:{key}", theirs.last_seq);
                        if let Some(seq) = insert_graph_doc_entry_tx(
                            &tx,
                            workspace.as_str(),
                            into_branch,
                            doc,
                            now_ms,
                            &GraphOp::NodeUpsert(GraphNodeUpsert {
                                id: key.clone(),
                                node_type: theirs.node_type.clone(),
                                title: theirs.title.clone(),
                                text: theirs.text.clone(),
                                tags: theirs.tags.clone(),
                                status: theirs.status.clone(),
                                meta_json: theirs.meta_json.clone(),
                            }),
                            Some(&merge_key),
                        )?
                        .1
                        {
                            let meta_json = merge_meta_json(
                                theirs.meta_json.as_deref(),
                                from_branch,
                                theirs.last_seq,
                                theirs.last_ts_ms,
                            );
                            insert_graph_node_version_tx(
                                &tx,
                                workspace.as_str(),
                                into_branch,
                                doc,
                                seq,
                                now_ms,
                                &key,
                                Some(theirs.node_type.as_str()),
                                theirs.title.as_deref(),
                                theirs.text.as_deref(),
                                &theirs.tags,
                                theirs.status.as_deref(),
                                Some(&meta_json),
                                theirs.deleted,
                            )?;
                            merged += 1;
                        } else {
                            skipped += 1;
                        }
                        continue;
                    }

                    // Diverged: create conflict.
                    if dry_run {
                        conflicts_created += 1;
                        continue;
                    }
                    let conflict_id = graph_conflict_create_node_tx(
                        &tx,
                        workspace.as_str(),
                        from_branch,
                        into_branch,
                        doc,
                        base_cutoff_seq,
                        &key,
                        base.as_ref(),
                        Some(theirs),
                        ours.as_ref(),
                        now_ms,
                    )?;
                    conflicts_created += 1;
                    conflict_ids.push(conflict_id);
                }
                GraphMergeCandidate::Edge { theirs, .. } => {
                    let key = GraphEdgeKey {
                        from: theirs.from.clone(),
                        rel: theirs.rel.clone(),
                        to: theirs.to.clone(),
                    };
                    let base =
                        graph_edge_get_tx(&tx, workspace.as_str(), &base_sources, doc, &key)?;
                    let ours =
                        graph_edge_get_tx(&tx, workspace.as_str(), &into_sources, doc, &key)?;

                    if graph_edge_semantic_eq(base.as_ref(), Some(theirs))
                        || graph_edge_semantic_eq(ours.as_ref(), Some(theirs))
                    {
                        skipped += 1;
                        continue;
                    }
                    if graph_edge_semantic_eq(base.as_ref(), ours.as_ref()) {
                        if dry_run {
                            merged += 1;
                            continue;
                        }
                        let key_str = format!("{}|{}|{}", key.from, key.rel, key.to);
                        let merge_key = format!(
                            "graph_merge:{from_branch}:{}:edge:{key_str}",
                            theirs.last_seq
                        );
                        if let Some(seq) = insert_graph_doc_entry_tx(
                            &tx,
                            workspace.as_str(),
                            into_branch,
                            doc,
                            now_ms,
                            &GraphOp::EdgeUpsert(GraphEdgeUpsert {
                                from: key.from.clone(),
                                rel: key.rel.clone(),
                                to: key.to.clone(),
                                meta_json: theirs.meta_json.clone(),
                            }),
                            Some(&merge_key),
                        )?
                        .1
                        {
                            let meta_json = merge_meta_json(
                                theirs.meta_json.as_deref(),
                                from_branch,
                                theirs.last_seq,
                                theirs.last_ts_ms,
                            );
                            insert_graph_edge_version_tx(
                                &tx,
                                workspace.as_str(),
                                into_branch,
                                doc,
                                seq,
                                now_ms,
                                &key.from,
                                &key.rel,
                                &key.to,
                                Some(&meta_json),
                                theirs.deleted,
                            )?;
                            merged += 1;
                        } else {
                            skipped += 1;
                        }
                        continue;
                    }

                    if dry_run {
                        conflicts_created += 1;
                        continue;
                    }
                    let conflict_id = graph_conflict_create_edge_tx(
                        &tx,
                        workspace.as_str(),
                        from_branch,
                        into_branch,
                        doc,
                        base_cutoff_seq,
                        &key,
                        base.as_ref(),
                        Some(theirs),
                        ours.as_ref(),
                        now_ms,
                    )?;
                    conflicts_created += 1;
                    conflict_ids.push(conflict_id);
                }
            }
        }

        if !dry_run && (merged > 0) {
            touch_document_tx(&tx, workspace.as_str(), into_branch, doc, now_ms)?;
        }

        let has_more = candidates.len() > processed;
        let next_cursor = if has_more {
            candidates
                .get(processed.saturating_sub(1))
                .map(|c| c.last_seq())
        } else {
            None
        };

        tx.commit()?;
        Ok(GraphMergeResult {
            merged,
            skipped,
            conflicts_created,
            conflict_ids,
            count: processed,
            next_cursor,
            has_more,
        })
    }

    pub fn graph_conflicts_list(
        &mut self,
        workspace: &WorkspaceId,
        into_branch: &str,
        doc: &str,
        status: Option<&str>,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<(Vec<GraphConflictSummary>, Option<i64>, bool), StoreError> {
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_created_at = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let status = status.unwrap_or("open");
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), into_branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let mut out = Vec::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT conflict_id, kind, key, status, created_at_ms
                FROM graph_conflicts
                WHERE workspace=?1 AND into_branch=?2 AND doc=?3 AND status=?4 AND created_at_ms < ?5
                ORDER BY created_at_ms DESC
                LIMIT ?6
                "#,
            )?;

            let mut rows = stmt.query(params![
                workspace.as_str(),
                into_branch,
                doc,
                status,
                before_created_at,
                limit + 1
            ])?;

            while let Some(row) = rows.next()? {
                out.push(GraphConflictSummary {
                    conflict_id: row.get(0)?,
                    kind: row.get(1)?,
                    key: row.get(2)?,
                    status: row.get(3)?,
                    created_at_ms: row.get(4)?,
                });
            }
        }

        let has_more = out.len() as i64 > limit;
        if has_more {
            out.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            out.last().map(|c| c.created_at_ms)
        } else {
            None
        };

        tx.commit()?;
        Ok((out, next_cursor, has_more))
    }

    pub fn graph_conflict_show(
        &mut self,
        workspace: &WorkspaceId,
        conflict_id: &str,
    ) -> Result<GraphConflictDetail, StoreError> {
        validate_conflict_id(conflict_id)?;

        let tx = self.conn.transaction()?;
        let row = graph_conflict_detail_row_tx(&tx, workspace.as_str(), conflict_id)?
            .ok_or(StoreError::UnknownConflict)?;
        tx.commit()?;

        Ok(row.into_detail(conflict_id))
    }

    pub fn graph_conflict_resolve(
        &mut self,
        workspace: &WorkspaceId,
        conflict_id: &str,
        resolution: &str,
    ) -> Result<GraphConflictResolveResult, StoreError> {
        validate_conflict_id(conflict_id)?;
        if resolution.trim().is_empty() {
            return Err(StoreError::InvalidInput("resolution must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = graph_conflict_detail_row_tx(&tx, workspace.as_str(), conflict_id)?
            .ok_or(StoreError::UnknownConflict)?;
        let detail = row.into_detail(conflict_id);
        if detail.status != "open" {
            return Err(StoreError::ConflictAlreadyResolved);
        }

        match resolution {
            "use_into" => {
                tx.execute(
                    "UPDATE graph_conflicts SET status='resolved', resolution=?3, resolved_at_ms=?4 WHERE workspace=?1 AND conflict_id=?2",
                    params![workspace.as_str(), conflict_id, resolution, now_ms],
                )?;
                tx.commit()?;
                return Ok(GraphConflictResolveResult {
                    conflict_id: conflict_id.to_string(),
                    status: "resolved".to_string(),
                    applied: false,
                    applied_seq: None,
                });
            }
            "use_from" => {}
            _ => {
                return Err(StoreError::InvalidInput(
                    "resolution must be use_from|use_into",
                ));
            }
        }

        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &detail.into_branch,
            &detail.doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;

        let source_event_id = format!("graph_conflict_resolve:{conflict_id}");

        let (applied, applied_seq) = match detail.kind.as_str() {
            "node" => {
                let Some(theirs) = detail.theirs_node.as_ref() else {
                    return Err(StoreError::InvalidInput(
                        "conflict has no theirs node snapshot",
                    ));
                };
                let op = GraphOp::NodeUpsert(GraphNodeUpsert {
                    id: theirs.id.clone(),
                    node_type: theirs.node_type.clone(),
                    title: theirs.title.clone(),
                    text: theirs.text.clone(),
                    tags: theirs.tags.clone(),
                    status: theirs.status.clone(),
                    meta_json: theirs.meta_json.clone(),
                });
                let inserted = insert_graph_doc_entry_tx(
                    &tx,
                    workspace.as_str(),
                    &detail.into_branch,
                    &detail.doc,
                    now_ms,
                    &op,
                    Some(&source_event_id),
                )?;
                match inserted.1 {
                    None => (false, None),
                    Some(seq) => {
                        let meta_json = merge_meta_json(
                            theirs.meta_json.as_deref(),
                            &detail.from_branch,
                            theirs.last_seq,
                            theirs.last_ts_ms,
                        );
                        insert_graph_node_version_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            seq,
                            now_ms,
                            &theirs.id,
                            Some(theirs.node_type.as_str()),
                            theirs.title.as_deref(),
                            theirs.text.as_deref(),
                            &theirs.tags,
                            theirs.status.as_deref(),
                            Some(&meta_json),
                            theirs.deleted,
                        )?;
                        touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            now_ms,
                        )?;
                        (true, Some(seq))
                    }
                }
            }
            "edge" => {
                let Some(theirs) = detail.theirs_edge.as_ref() else {
                    return Err(StoreError::InvalidInput(
                        "conflict has no theirs edge snapshot",
                    ));
                };
                let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
                    from: theirs.from.clone(),
                    rel: theirs.rel.clone(),
                    to: theirs.to.clone(),
                    meta_json: theirs.meta_json.clone(),
                });
                let inserted = insert_graph_doc_entry_tx(
                    &tx,
                    workspace.as_str(),
                    &detail.into_branch,
                    &detail.doc,
                    now_ms,
                    &op,
                    Some(&source_event_id),
                )?;
                match inserted.1 {
                    None => (false, None),
                    Some(seq) => {
                        let meta_json = merge_meta_json(
                            theirs.meta_json.as_deref(),
                            &detail.from_branch,
                            theirs.last_seq,
                            theirs.last_ts_ms,
                        );
                        insert_graph_edge_version_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            seq,
                            now_ms,
                            &theirs.from,
                            &theirs.rel,
                            &theirs.to,
                            Some(&meta_json),
                            theirs.deleted,
                        )?;
                        touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            now_ms,
                        )?;
                        (true, Some(seq))
                    }
                }
            }
            _ => return Err(StoreError::InvalidInput("unknown conflict kind")),
        };

        tx.execute(
            "UPDATE graph_conflicts SET status='resolved', resolution=?3, resolved_at_ms=?4 WHERE workspace=?1 AND conflict_id=?2",
            params![workspace.as_str(), conflict_id, resolution, now_ms],
        )?;

        tx.commit()?;
        Ok(GraphConflictResolveResult {
            conflict_id: conflict_id.to_string(),
            status: "resolved".to_string(),
            applied,
            applied_seq,
        })
    }

    pub fn doc_ingest_task_event(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        event: &EventRow,
    ) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, event.ts_ms)?;
        let inserted = ingest_task_event_tx(&tx, workspace.as_str(), branch, doc, event)?;
        tx.commit()?;
        Ok(inserted)
    }

    pub fn branch_checkout_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn branch_checkout_set(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<(Option<String>, String), StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let previous = tx
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        tx.execute(
            r#"
            INSERT INTO branch_checkout(workspace, branch, updated_at_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace) DO UPDATE SET branch=excluded.branch, updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace.as_str(), branch, now_ms],
        )?;

        tx.commit()?;
        Ok((previous, branch.to_string()))
    }

    pub fn branch_create(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
        from: Option<&str>,
    ) -> Result<BranchInfo, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if branch_exists_tx(&tx, workspace.as_str(), name)? {
            return Err(StoreError::BranchAlreadyExists);
        }

        let base_branch = match from {
            Some(v) if !v.trim().is_empty() => v.to_string(),
            Some(_) => return Err(StoreError::InvalidInput("from must not be empty")),
            None => {
                if let Some(branch) = branch_checkout_get_tx(&tx, workspace.as_str())? {
                    branch
                } else {
                    let _ = bootstrap_default_branch_tx(&tx, workspace.as_str(), now_ms)?;
                    if let Some(branch) = branch_checkout_get_tx(&tx, workspace.as_str())? {
                        branch
                    } else if branch_exists_tx(&tx, workspace.as_str(), DEFAULT_BRANCH)? {
                        DEFAULT_BRANCH.to_string()
                    } else {
                        return Err(StoreError::InvalidInput(
                            "from is required when no checkout branch is set",
                        ));
                    }
                }
            }
        };

        if !branch_exists_tx(&tx, workspace.as_str(), &base_branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let base_seq = doc_entries_head_seq_tx(&tx, workspace.as_str())?.unwrap_or(0);

        tx.execute(
            r#"
            INSERT INTO branches(workspace, name, base_branch, base_seq, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                workspace.as_str(),
                name,
                base_branch.as_str(),
                base_seq,
                now_ms
            ],
        )?;

        tx.commit()?;
        Ok(BranchInfo {
            name: name.to_string(),
            base_branch: Some(base_branch),
            base_seq: Some(base_seq),
            created_at_ms: Some(now_ms),
        })
    }

    pub fn branch_rename(
        &mut self,
        workspace: &WorkspaceId,
        from: &str,
        to: &str,
    ) -> Result<(String, String), StoreError> {
        let from = from.trim();
        let to = to.trim();
        if from.is_empty() {
            return Err(StoreError::InvalidInput("from must not be empty"));
        }
        if to.is_empty() {
            return Err(StoreError::InvalidInput("to must not be empty"));
        }
        if from == to {
            return Err(StoreError::InvalidInput("from and to must differ"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), from)? {
            return Err(StoreError::UnknownBranch);
        }
        if branch_exists_tx(&tx, workspace.as_str(), to)? {
            return Err(StoreError::BranchAlreadyExists);
        }

        tx.execute(
            "UPDATE branches SET name=?1 WHERE workspace=?2 AND name=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE branches SET base_branch=?1 WHERE workspace=?2 AND base_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE branch_checkout SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE reasoning_refs SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE documents SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE doc_entries SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_node_versions SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_edge_versions SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_conflicts SET from_branch=?1 WHERE workspace=?2 AND from_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE graph_conflicts SET into_branch=?1 WHERE workspace=?2 AND into_branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_refs SET ref=?1 WHERE workspace=?2 AND ref=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_refs SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_reflog SET ref=?1 WHERE workspace=?2 AND ref=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_reflog SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;
        tx.execute(
            "UPDATE vcs_tags SET branch=?1 WHERE workspace=?2 AND branch=?3",
            params![to, workspace.as_str(), from],
        )?;

        tx.commit()?;
        Ok((from.to_string(), to.to_string()))
    }

    pub fn branch_delete(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<bool, StoreError> {
        let branch = name.trim();
        if branch.is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }

        let tx = self.conn.transaction()?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        if let Some(current) = branch_checkout_get_tx(&tx, workspace.as_str())? {
            if current == branch {
                return Err(StoreError::InvalidInput(
                    "cannot delete the currently checked-out branch",
                ));
            }
        }

        let has_children = tx
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND base_branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_row| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if has_children {
            return Err(StoreError::InvalidInput(
                "branch has dependent branches; delete or rebase them first",
            ));
        }

        let referenced = tx
            .query_row(
                "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_row| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if referenced {
            return Err(StoreError::InvalidInput(
                "branch is referenced by reasoning refs; move them before deletion",
            ));
        }

        tx.execute(
            "DELETE FROM vcs_refs WHERE workspace=?1 AND ref=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM vcs_reflog WHERE workspace=?1 AND ref=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM vcs_tags WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_conflicts WHERE workspace=?1 AND (from_branch=?2 OR into_branch=?2)",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_edge_versions WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM graph_node_versions WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM doc_entries WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        tx.execute(
            "DELETE FROM documents WHERE workspace=?1 AND branch=?2",
            params![workspace.as_str(), branch],
        )?;
        let deleted = tx.execute(
            "DELETE FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace.as_str(), branch],
        )?;

        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn branch_list(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
    ) -> Result<Vec<BranchInfo>, StoreError> {
        use std::collections::HashMap;

        let limit = limit.clamp(1, 500);
        let mut map: HashMap<String, BranchInfo> = HashMap::new();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT name, base_branch, base_seq, created_at_ms
            FROM branches
            WHERE workspace=?1
            ORDER BY name ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str()], |row| {
            Ok(BranchInfo {
                name: row.get::<_, String>(0)?,
                base_branch: Some(row.get::<_, String>(1)?),
                base_seq: Some(row.get::<_, i64>(2)?),
                created_at_ms: Some(row.get::<_, i64>(3)?),
            })
        })?;
        for row in rows {
            let info = row?;
            map.insert(info.name.clone(), info);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM reasoning_refs WHERE workspace=?1")?;
        let refs = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in refs {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM doc_entries WHERE workspace=?1")?;
        let entries = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in entries {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut names = map.keys().cloned().collect::<Vec<_>>();
        names.sort();
        let mut out = Vec::new();
        for name in names.into_iter().take(limit) {
            if let Some(info) = map.remove(&name) {
                out.push(info);
            }
        }
        Ok(out)
    }

    pub fn branch_exists(&self, workspace: &WorkspaceId, branch: &str) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        Ok(false)
    }

    pub fn steps_decompose(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        parent_path: Option<&StepPath>,
        steps: Vec<NewStep>,
    ) -> Result<DecomposeResult, StoreError> {
        if steps.is_empty() {
            return Err(StoreError::InvalidInput("steps must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;

        let parent_step_id = match parent_path {
            None => None,
            Some(path) => Some(resolve_step_id_tx(&tx, workspace.as_str(), task_id, path)?),
        };

        let max_ordinal: Option<i64> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL",
                    params![workspace.as_str(), task_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                    params![workspace.as_str(), task_id, parent_step_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
        };

        let mut next_ordinal = max_ordinal.unwrap_or(-1) + 1;
        let mut created_steps = Vec::with_capacity(steps.len());

        for step in steps {
            let seq = next_counter_tx(&tx, workspace.as_str(), "step_seq")?;
            let step_id = format!("STEP-{seq:08X}");
            let ordinal = next_ordinal;
            next_ordinal += 1;

            tx.execute(
                r#"
                INSERT INTO steps(workspace,task_id,step_id,parent_step_id,ordinal,title,completed,criteria_confirmed,tests_confirmed,created_at_ms,updated_at_ms)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    parent_step_id,
                    ordinal,
                    step.title,
                    0i64,
                    0i64,
                    0i64,
                    now_ms,
                    now_ms
                ],
            )?;

            for (i, text) in step.success_criteria.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }

            let path = match parent_path {
                None => StepPath::root(ordinal as usize).to_string(),
                Some(parent) => parent.child(ordinal as usize).to_string(),
            };
            created_steps.push(StepRef { step_id, path });
        }

        let parent_path_str = parent_path.map(|p| p.to_string());
        let event_payload_json =
            build_steps_added_payload(task_id, parent_path_str.as_deref(), &created_steps);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            parent_path_str,
            "steps_added",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let mut graph_touched = false;
        let task_title = task_title_tx(&tx, workspace.as_str(), task_id)?;
        graph_touched |= Self::project_task_graph_task_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &task_title,
            now_ms,
        )?;

        let parent_node_id = if let Some(parent_step_id) = parent_step_id.clone() {
            let parent_path = parent_path
                .map(|p| p.to_string())
                .unwrap_or_else(|| "s:?".to_string());
            let parent_ref = StepRef {
                step_id: parent_step_id,
                path: parent_path,
            };
            let (parent_title, parent_completed) =
                step_snapshot_tx(&tx, workspace.as_str(), task_id, &parent_ref.step_id)?;
            graph_touched |= Self::project_task_graph_step_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                task_id,
                &parent_ref,
                &parent_title,
                parent_completed,
                now_ms,
            )?;
            step_graph_node_id(&parent_ref.step_id)
        } else {
            task_graph_node_id(task_id)
        };

        for step in created_steps.iter() {
            let (title, completed) =
                step_snapshot_tx(&tx, workspace.as_str(), task_id, &step.step_id)?;
            graph_touched |= Self::project_task_graph_step_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                task_id,
                step,
                &title,
                completed,
                now_ms,
            )?;

            let step_node_id = step_graph_node_id(&step.step_id);
            graph_touched |= Self::project_task_graph_contains_edge_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &parent_node_id,
                &step_node_id,
                now_ms,
            )?;
        }
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(DecomposeResult {
            task_revision,
            steps: created_steps,
            event,
        })
    }

    pub fn step_define(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        title: Option<String>,
        success_criteria: Option<Vec<String>>,
        tests: Option<Vec<String>>,
        blockers: Option<Vec<String>>,
    ) -> Result<StepOpResult, StoreError> {
        if title.is_none() && success_criteria.is_none() && tests.is_none() && blockers.is_none() {
            return Err(StoreError::InvalidInput("no fields to define"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let mut fields = Vec::new();

        if let Some(title) = title {
            tx.execute(
                "UPDATE steps SET title=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, title, now_ms],
            )?;
            fields.push("title");
        }

        if let Some(items) = success_criteria {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET criteria_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("success_criteria");
        }

        if let Some(items) = tests {
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET tests_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("tests");
        }

        if let Some(items) = blockers {
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            fields.push("blockers");
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_defined_payload(task_id, &step_ref, &fields);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_defined",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_note(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        note: String,
    ) -> Result<StepOpResult, StoreError> {
        if note.trim().is_empty() {
            return Err(StoreError::InvalidInput("note must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        tx.execute(
            "INSERT INTO step_notes(workspace, task_id, step_id, ts_ms, note) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workspace.as_str(), task_id, step_id, now_ms, &note],
        )?;
        let note_seq = tx.last_insert_rowid();

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_noted_payload(task_id, &step_ref, note_seq);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_noted",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        // Mirror the human-authored note content into the reasoning notes document (single organism invariant).
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;
        let meta_json =
            build_step_noted_mirror_meta_json(task_id, &step_ref, note_seq, &event.event_id());
        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.notes_doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                meta_json,
                &note
            ],
        )?;
        touch_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            now_ms,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_verify(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        criteria_confirmed: Option<bool>,
        tests_confirmed: Option<bool>,
        security_confirmed: Option<bool>,
        perf_confirmed: Option<bool>,
        docs_confirmed: Option<bool>,
    ) -> Result<StepOpResult, StoreError> {
        if criteria_confirmed.is_none()
            && tests_confirmed.is_none()
            && security_confirmed.is_none()
            && perf_confirmed.is_none()
            && docs_confirmed.is_none()
        {
            return Err(StoreError::InvalidInput("no checkpoints to verify"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
        if let Some(v) = criteria_confirmed {
            tx.execute(
                "UPDATE steps SET criteria_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = tests_confirmed {
            tx.execute(
                "UPDATE steps SET tests_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = security_confirmed {
            tx.execute(
                "UPDATE steps SET security_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = perf_confirmed {
            tx.execute(
                "UPDATE steps SET perf_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = docs_confirmed {
            tx.execute(
                "UPDATE steps SET docs_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_verified_payload(
            task_id,
            &step_ref,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        );
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_verified",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_close(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        criteria_confirmed: Option<bool>,
        tests_confirmed: Option<bool>,
        security_confirmed: Option<bool>,
        perf_confirmed: Option<bool>,
        docs_confirmed: Option<bool>,
    ) -> Result<StepCloseResult, StoreError> {
        if criteria_confirmed.is_none()
            && tests_confirmed.is_none()
            && security_confirmed.is_none()
            && perf_confirmed.is_none()
            && docs_confirmed.is_none()
        {
            return Err(StoreError::InvalidInput("no checkpoints to verify"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let row = tx
            .query_row(
                "SELECT completed, criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;

        let Some((completed, _, _, _, _, _)) = row else {
            return Err(StoreError::StepNotFound);
        };
        if completed != 0 {
            return Err(StoreError::InvalidInput("step already completed"));
        }

        if let Some(v) = criteria_confirmed {
            tx.execute(
                "UPDATE steps SET criteria_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = tests_confirmed {
            tx.execute(
                "UPDATE steps SET tests_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = security_confirmed {
            tx.execute(
                "UPDATE steps SET security_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = perf_confirmed {
            tx.execute(
                "UPDATE steps SET perf_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }
        if let Some(v) = docs_confirmed {
            tx.execute(
                "UPDATE steps SET docs_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }

        let (criteria_now, tests_now, security_now, perf_now, docs_now) = tx
            .query_row(
                "SELECT criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::StepNotFound)?;

        let require_security = security_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?;
        let require_perf = perf_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?;
        let require_docs = docs_confirmed.is_some()
            || checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?;

        if criteria_now == 0
            || tests_now == 0
            || (require_security && security_now == 0)
            || (require_perf && perf_now == 0)
            || (require_docs && docs_now == 0)
        {
            return Err(StoreError::CheckpointsNotConfirmed {
                criteria: criteria_now == 0,
                tests: tests_now == 0,
                security: require_security && security_now == 0,
                perf: require_perf && perf_now == 0,
                docs: require_docs && docs_now == 0,
            });
        }

        tx.execute(
            "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let verify_payload_json = build_step_verified_payload(
            task_id,
            &step_ref,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        );
        let verify_event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_verified",
            &verify_payload_json,
        )?;
        let done_payload_json = build_step_done_payload(task_id, &step_ref);
        let done_event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_done",
            &done_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &verify_event,
        )?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &done_event,
        )?;

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &done_event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepCloseResult {
            task_revision,
            step: step_ref,
            events: vec![verify_event, done_event],
        })
    }

    pub fn step_done(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let row = tx
            .query_row(
                "SELECT completed, criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            completed,
            criteria_confirmed,
            tests_confirmed,
            security_confirmed,
            perf_confirmed,
            docs_confirmed,
        )) = row
        else {
            return Err(StoreError::StepNotFound);
        };

        if completed != 0 {
            return Err(StoreError::InvalidInput("step already completed"));
        }

        let require_security =
            checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?;
        let require_perf =
            checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?;
        let require_docs =
            checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?;

        if criteria_confirmed == 0
            || tests_confirmed == 0
            || (require_security && security_confirmed == 0)
            || (require_perf && perf_confirmed == 0)
            || (require_docs && docs_confirmed == 0)
        {
            return Err(StoreError::CheckpointsNotConfirmed {
                criteria: criteria_confirmed == 0,
                tests: tests_confirmed == 0,
                security: require_security && security_confirmed == 0,
                perf: require_perf && perf_confirmed == 0,
                docs: require_docs && docs_confirmed == 0,
            });
        }

        tx.execute(
            "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_done_payload(task_id, &step_ref);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_done",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_detail(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        step_id: Option<&str>,
        path: Option<&StepPath>,
    ) -> Result<StepDetail, StoreError> {
        let tx = self.conn.transaction()?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
        let row = tx
            .query_row(
                r#"
                SELECT title, criteria_confirmed, tests_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed,
                       completed, blocked, block_reason
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((title, criteria, tests, security, perf, docs, completed, blocked, block_reason)) =
            row
        else {
            return Err(StoreError::StepNotFound);
        };

        let success_criteria =
            step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_criteria")?;
        let tests_list = step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_tests")?;
        let blockers = step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_blockers")?;

        tx.commit()?;
        Ok(StepDetail {
            step_id,
            path,
            title,
            success_criteria,
            tests: tests_list,
            blockers,
            criteria_confirmed: criteria != 0,
            tests_confirmed: tests != 0,
            security_confirmed: security != 0,
            perf_confirmed: perf != 0,
            docs_confirmed: docs != 0,
            completed: completed != 0,
            blocked: blocked != 0,
            block_reason,
        })
    }

    pub fn task_node_detail(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        node_id: Option<&str>,
        parent_path: Option<&StepPath>,
        ordinal: Option<i64>,
    ) -> Result<TaskNodeDetail, StoreError> {
        let tx = self.conn.transaction()?;
        let (node_id, path, _parent_step_id, _ordinal) = resolve_task_node_selector_tx(
            &tx,
            workspace.as_str(),
            task_id,
            node_id,
            parent_path,
            ordinal,
        )?;
        let row = tx
            .query_row(
                r#"
                SELECT node_id, task_id, parent_step_id, ordinal, title, status, status_manual,
                       priority, blocked, description, context, created_at_ms, updated_at_ms
                FROM task_nodes
                WHERE workspace=?1 AND task_id=?2 AND node_id=?3
                "#,
                params![workspace.as_str(), task_id, node_id],
                |row| {
                    Ok(TaskNodeRow {
                        node_id: row.get(0)?,
                        task_id: row.get(1)?,
                        parent_step_id: row.get(2)?,
                        ordinal: row.get(3)?,
                        title: row.get(4)?,
                        status: row.get(5)?,
                        status_manual: row.get::<_, i64>(6)? != 0,
                        priority: row.get(7)?,
                        blocked: row.get::<_, i64>(8)? != 0,
                        description: row.get(9)?,
                        context: row.get(10)?,
                        created_at_ms: row.get(11)?,
                        updated_at_ms: row.get(12)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Err(StoreError::UnknownId);
        };

        let blockers = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "blockers",
        )?;
        let dependencies = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "dependencies",
        )?;
        let next_steps = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "next_steps",
        )?;
        let problems = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "problems",
        )?;
        let risks =
            task_items_list_tx(&tx, workspace.as_str(), "task_node", &row.node_id, "risks")?;
        let success_criteria = task_items_list_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &row.node_id,
            "success_criteria",
        )?;

        tx.commit()?;
        Ok(TaskNodeDetail {
            row,
            path,
            blockers,
            dependencies,
            next_steps,
            problems,
            risks,
            success_criteria,
        })
    }

    pub fn task_node_add(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        parent_path: &StepPath,
        title: String,
        status: String,
        status_manual: bool,
        priority: String,
        blocked: bool,
        description: Option<String>,
        context: Option<String>,
        items: TaskNodeItems,
        record_undo: bool,
    ) -> Result<TaskNodeOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let parent_step_id = resolve_step_id_tx(&tx, workspace.as_str(), task_id, parent_path)?;
        let ordinal: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal), -1) FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            params![workspace.as_str(), task_id, parent_step_id],
            |row| row.get(0),
        )?;
        let ordinal = ordinal + 1;
        let seq = next_counter_tx(&tx, workspace.as_str(), "task_node_seq")?;
        let node_id = format!("NODE-{seq:08X}");

        tx.execute(
            r#"
            INSERT INTO task_nodes(
                workspace, node_id, task_id, parent_step_id, ordinal,
                title, status, status_manual, priority, blocked, description, context,
                created_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                workspace.as_str(),
                node_id,
                task_id,
                parent_step_id,
                ordinal,
                title,
                status,
                if status_manual { 1i64 } else { 0i64 },
                priority,
                if blocked { 1i64 } else { 0i64 },
                description,
                context,
                now_ms,
                now_ms
            ],
        )?;

        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "blockers",
            &items.blockers,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "dependencies",
            &items.dependencies,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "next_steps",
            &items.next_steps,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "problems",
            &items.problems,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "risks",
            &items.risks,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "success_criteria",
            &items.success_criteria,
        )?;

        let path = task_node_path_for_parent_tx(
            &tx,
            workspace.as_str(),
            task_id,
            &parent_step_id,
            ordinal,
        )?;
        let event_payload_json =
            build_task_node_added_payload(task_id, &node_id, &path, &parent_path.to_string());
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "task_node_added",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            let after_snapshot = json!({
                "task": task_id,
                "node_id": node_id.clone(),
                "path": path.clone(),
                "title": title,
                "status": status,
                "status_manual": status_manual,
                "priority": priority,
                "blocked": blocked,
                "description": description,
                "context": context,
                "blockers": items.blockers,
                "dependencies": items.dependencies,
                "next_steps": items.next_steps,
                "problems": items.problems,
                "risks": items.risks,
                "success_criteria": items.success_criteria
            });
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "task_node_add",
                &event_payload_json,
                None,
                Some(&after_snapshot.to_string()),
                false,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(TaskNodeOpResult {
            task_revision,
            node: TaskNodeRef { node_id, path },
            event,
        })
    }

    pub fn task_node_patch(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        node_id: Option<&str>,
        parent_path: Option<&StepPath>,
        ordinal: Option<i64>,
        patch: TaskNodePatch,
        record_undo: bool,
    ) -> Result<TaskNodeOpResult, StoreError> {
        if patch.title.is_none()
            && patch.status.is_none()
            && patch.status_manual.is_none()
            && patch.priority.is_none()
            && patch.blocked.is_none()
            && patch.description.is_none()
            && patch.context.is_none()
            && patch.blockers.is_none()
            && patch.dependencies.is_none()
            && patch.next_steps.is_none()
            && patch.problems.is_none()
            && patch.risks.is_none()
            && patch.success_criteria.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (node_id, path, _parent_step_id, _ordinal) = resolve_task_node_selector_tx(
            &tx,
            workspace.as_str(),
            task_id,
            node_id,
            parent_path,
            ordinal,
        )?;

        let row = tx
            .query_row(
                r#"
                SELECT title, status, status_manual, priority, blocked, description, context
                FROM task_nodes
                WHERE workspace=?1 AND task_id=?2 AND node_id=?3
                "#,
                params![workspace.as_str(), task_id, node_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            current_title,
            current_status,
            current_status_manual,
            current_priority,
            current_blocked,
            current_description,
            current_context,
        )) = row
        else {
            return Err(StoreError::UnknownId);
        };

        let before_items = TaskNodeItems {
            blockers: task_items_list_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "blockers",
            )?,
            dependencies: task_items_list_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "dependencies",
            )?,
            next_steps: task_items_list_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "next_steps",
            )?,
            problems: task_items_list_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "problems",
            )?,
            risks: task_items_list_tx(&tx, workspace.as_str(), "task_node", &node_id, "risks")?,
            success_criteria: task_items_list_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "success_criteria",
            )?,
        };

        let before_snapshot = json!({
            "task": task_id,
            "node_id": node_id.clone(),
            "path": path.clone(),
            "title": current_title,
            "status": current_status,
            "status_manual": current_status_manual != 0,
            "priority": current_priority,
            "blocked": current_blocked != 0,
            "description": current_description,
            "context": current_context,
            "blockers": before_items.blockers.clone(),
            "dependencies": before_items.dependencies.clone(),
            "next_steps": before_items.next_steps.clone(),
            "problems": before_items.problems.clone(),
            "risks": before_items.risks.clone(),
            "success_criteria": before_items.success_criteria.clone()
        });

        let has_title = patch.title.is_some();
        let has_status = patch.status.is_some();
        let has_status_manual = patch.status_manual.is_some();
        let has_priority = patch.priority.is_some();
        let has_blocked = patch.blocked.is_some();
        let has_description = patch.description.is_some();
        let has_context = patch.context.is_some();
        let has_blockers = patch.blockers.is_some();
        let has_dependencies = patch.dependencies.is_some();
        let has_next_steps = patch.next_steps.is_some();
        let has_problems = patch.problems.is_some();
        let has_risks = patch.risks.is_some();
        let has_success_criteria = patch.success_criteria.is_some();

        let mut next_title = current_title;
        let mut next_status = current_status;
        let mut next_status_manual = current_status_manual != 0;
        let mut next_priority = current_priority;
        let mut next_blocked = current_blocked != 0;
        let mut next_description = current_description;
        let mut next_context = current_context;

        if let Some(value) = patch.title {
            next_title = value;
        }
        if let Some(value) = patch.status {
            next_status = value;
        }
        if let Some(value) = patch.status_manual {
            next_status_manual = value;
        }
        if let Some(value) = patch.priority {
            next_priority = value;
        }
        if let Some(value) = patch.blocked {
            next_blocked = value;
        }
        if let Some(value) = patch.description {
            next_description = value;
        }
        if let Some(value) = patch.context {
            next_context = value;
        }

        tx.execute(
            r#"
            UPDATE task_nodes
            SET title=?4, status=?5, status_manual=?6, priority=?7, blocked=?8,
                description=?9, context=?10, updated_at_ms=?11
            WHERE workspace=?1 AND task_id=?2 AND node_id=?3
            "#,
            params![
                workspace.as_str(),
                task_id,
                node_id,
                next_title,
                next_status,
                if next_status_manual { 1i64 } else { 0i64 },
                next_priority,
                if next_blocked { 1i64 } else { 0i64 },
                next_description,
                next_context,
                now_ms
            ],
        )?;

        let next_items = TaskNodeItems {
            blockers: patch
                .blockers
                .unwrap_or_else(|| before_items.blockers.clone()),
            dependencies: patch
                .dependencies
                .unwrap_or_else(|| before_items.dependencies.clone()),
            next_steps: patch
                .next_steps
                .unwrap_or_else(|| before_items.next_steps.clone()),
            problems: patch
                .problems
                .unwrap_or_else(|| before_items.problems.clone()),
            risks: patch.risks.unwrap_or_else(|| before_items.risks.clone()),
            success_criteria: patch
                .success_criteria
                .unwrap_or_else(|| before_items.success_criteria.clone()),
        };

        if has_blockers {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "blockers",
                &next_items.blockers,
            )?;
        }
        if has_dependencies {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "dependencies",
                &next_items.dependencies,
            )?;
        }
        if has_next_steps {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "next_steps",
                &next_items.next_steps,
            )?;
        }
        if has_problems {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "problems",
                &next_items.problems,
            )?;
        }
        if has_risks {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "risks",
                &next_items.risks,
            )?;
        }
        if has_success_criteria {
            task_items_replace_tx(
                &tx,
                workspace.as_str(),
                "task_node",
                &node_id,
                "success_criteria",
                &next_items.success_criteria,
            )?;
        }

        let mut fields = Vec::new();
        if has_title {
            fields.push("title");
        }
        if has_status {
            fields.push("status");
        }
        if has_status_manual {
            fields.push("status_manual");
        }
        if has_priority {
            fields.push("priority");
        }
        if has_blocked {
            fields.push("blocked");
        }
        if has_description {
            fields.push("description");
        }
        if has_context {
            fields.push("context");
        }
        if has_blockers {
            fields.push("blockers");
        }
        if has_dependencies {
            fields.push("dependencies");
        }
        if has_next_steps {
            fields.push("next_steps");
        }
        if has_problems {
            fields.push("problems");
        }
        if has_risks {
            fields.push("risks");
        }
        if has_success_criteria {
            fields.push("success_criteria");
        }

        let event_payload_json = build_task_node_defined_payload(task_id, &node_id, &path, &fields);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "task_node_defined",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            let after_snapshot = json!({
                "task": task_id,
                "node_id": node_id.clone(),
                "path": path.clone(),
                "title": next_title,
                "status": next_status,
                "status_manual": next_status_manual,
                "priority": next_priority,
                "blocked": next_blocked,
                "description": next_description,
                "context": next_context,
                "blockers": next_items.blockers.clone(),
                "dependencies": next_items.dependencies.clone(),
                "next_steps": next_items.next_steps.clone(),
                "problems": next_items.problems.clone(),
                "risks": next_items.risks.clone(),
                "success_criteria": next_items.success_criteria.clone()
            });
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "task_node_patch",
                &event_payload_json,
                Some(&before_snapshot.to_string()),
                Some(&after_snapshot.to_string()),
                true,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(TaskNodeOpResult {
            task_revision,
            node: TaskNodeRef { node_id, path },
            event,
        })
    }

    pub fn task_node_delete(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        node_id: Option<&str>,
        parent_path: Option<&StepPath>,
        ordinal: Option<i64>,
        record_undo: bool,
    ) -> Result<TaskNodeOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (node_id, path, _parent_step_id, _ordinal) = resolve_task_node_selector_tx(
            &tx,
            workspace.as_str(),
            task_id,
            node_id,
            parent_path,
            ordinal,
        )?;

        tx.execute(
            "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
            params![workspace.as_str(), node_id],
        )?;
        tx.execute(
            "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND node_id=?3",
            params![workspace.as_str(), task_id, node_id],
        )?;

        let event_payload_json = build_task_node_deleted_payload(task_id, &node_id, &path);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "task_node_deleted",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "task_node_delete",
                &event_payload_json,
                None,
                None,
                false,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(TaskNodeOpResult {
            task_revision,
            node: TaskNodeRef { node_id, path },
            event,
        })
    }

    pub fn task_detail_patch(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        kind: TaskKind,
        patch: TaskDetailPatch,
        event_type: String,
        event_payload_json: String,
        record_undo: bool,
    ) -> Result<(i64, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let (
            mut title,
            mut description,
            mut context,
            mut priority,
            mut contract,
            mut contract_json,
        );
        let (mut domain, mut phase, mut component, mut assignee);

        match kind {
            TaskKind::Plan => {
                let row = tx
                    .query_row(
                        "SELECT title, description, context, priority, contract, contract_json FROM plans WHERE workspace=?1 AND id=?2",
                        params![workspace.as_str(), task_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, Option<String>>(5)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((t, d, c, p, ct, cj)) = row else {
                    return Err(StoreError::UnknownId);
                };
                title = t;
                description = d;
                context = c;
                priority = p;
                contract = ct;
                contract_json = cj;
                domain = None;
                phase = None;
                component = None;
                assignee = None;
            }
            TaskKind::Task => {
                let row = tx
                    .query_row(
                        "SELECT title, description, context, priority, domain, phase, component, assignee FROM tasks WHERE workspace=?1 AND id=?2",
                        params![workspace.as_str(), task_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, Option<String>>(5)?,
                                row.get::<_, Option<String>>(6)?,
                                row.get::<_, Option<String>>(7)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((t, d, c, p, dm, ph, comp, asg)) = row else {
                    return Err(StoreError::UnknownId);
                };
                title = t;
                description = d;
                context = c;
                priority = p;
                domain = dm;
                phase = ph;
                component = comp;
                assignee = asg;
                contract = None;
                contract_json = None;
            }
        }

        let tags = task_items_list_tx(&tx, workspace.as_str(), kind.as_str(), task_id, "tags")?;
        let depends_on = task_items_list_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            task_id,
            "depends_on",
        )?;

        let before_snapshot = json!({
            "kind": kind.as_str(),
            "task": task_id,
            "title": title,
            "description": description,
            "context": context,
            "priority": priority,
            "contract": contract,
            "contract_data": parse_json_or_null(contract_json.clone()),
            "domain": domain,
            "phase": phase,
            "component": component,
            "assignee": assignee,
            "tags": tags,
            "depends_on": depends_on
        });

        if let Some(v) = patch.title {
            title = v;
        }
        if let Some(v) = patch.description {
            description = v;
        }
        if let Some(v) = patch.context {
            context = v;
        }
        if let Some(v) = patch.priority {
            priority = v;
        }
        if let Some(v) = patch.contract {
            contract = v;
        }
        if let Some(v) = patch.contract_json {
            contract_json = v;
        }
        if let Some(v) = patch.domain {
            domain = v;
        }
        if let Some(v) = patch.phase {
            phase = v;
        }
        if let Some(v) = patch.component {
            component = v;
        }
        if let Some(v) = patch.assignee {
            assignee = v;
        }
        let next_tags = patch.tags.unwrap_or_else(|| tags.clone());
        let next_depends = patch.depends_on.unwrap_or_else(|| depends_on.clone());

        let revision = match kind {
            TaskKind::Plan => {
                bump_plan_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?
            }
            TaskKind::Task => {
                bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?
            }
        };

        match kind {
            TaskKind::Plan => {
                tx.execute(
                    r#"
                    UPDATE plans
                    SET title=?3, description=?4, context=?5, priority=?6, contract=?7, contract_json=?8, updated_at_ms=?9
                    WHERE workspace=?1 AND id=?2
                    "#,
                    params![
                        workspace.as_str(),
                        task_id,
                        title,
                        description,
                        context,
                        priority,
                        contract,
                        contract_json,
                        now_ms
                    ],
                )?;
            }
            TaskKind::Task => {
                tx.execute(
                    r#"
                    UPDATE tasks
                    SET title=?3, description=?4, context=?5, priority=?6,
                        domain=?7, phase=?8, component=?9, assignee=?10, updated_at_ms=?11
                    WHERE workspace=?1 AND id=?2
                    "#,
                    params![
                        workspace.as_str(),
                        task_id,
                        title,
                        description,
                        context,
                        priority,
                        domain,
                        phase,
                        component,
                        assignee,
                        now_ms
                    ],
                )?;
            }
        }

        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            task_id,
            "tags",
            &next_tags,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            task_id,
            "depends_on",
            &next_depends,
        )?;

        let after_snapshot = json!({
            "kind": kind.as_str(),
            "task": task_id,
            "title": title,
            "description": description,
            "context": context,
            "priority": priority,
            "contract": contract,
            "contract_data": parse_json_or_null(contract_json.clone()),
            "domain": domain,
            "phase": phase,
            "component": component,
            "assignee": assignee,
            "tags": next_tags,
            "depends_on": next_depends
        });

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, task_id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                None,
                "task_detail_patch",
                &event_payload_json,
                Some(&before_snapshot.to_string()),
                Some(&after_snapshot.to_string()),
                true,
                now_ms,
            )?;
        }

        if matches!(kind, TaskKind::Task) {
            let touched = Self::project_task_graph_task_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                task_id,
                &title,
                now_ms,
            )?;
            if touched {
                touch_document_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.graph_doc,
                    now_ms,
                )?;
            }
        }

        tx.commit()?;
        Ok((revision, event))
    }

    pub fn step_patch(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        patch: StepPatch,
        event_payload_json: String,
        record_undo: bool,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let (detail, before_completed_at_ms) = {
            let row = tx
                .query_row(
                    r#"
                    SELECT title, criteria_confirmed, tests_confirmed, security_confirmed,
                           perf_confirmed, docs_confirmed, completed, completed_at_ms, blocked, block_reason
                    FROM steps
                    WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                    "#,
                    params![workspace.as_str(), task_id, step_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<i64>>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, Option<String>>(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                title,
                criteria,
                tests,
                security,
                perf,
                docs,
                completed,
                completed_at_ms,
                blocked,
                block_reason,
            )) = row
            else {
                return Err(StoreError::StepNotFound);
            };
            let detail = StepDetail {
                step_id: step_id.clone(),
                path: path.clone(),
                title,
                success_criteria: step_items_list_tx(
                    &tx,
                    workspace.as_str(),
                    &step_id,
                    "step_criteria",
                )?,
                tests: step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_tests")?,
                blockers: step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_blockers")?,
                criteria_confirmed: criteria != 0,
                tests_confirmed: tests != 0,
                security_confirmed: security != 0,
                perf_confirmed: perf != 0,
                docs_confirmed: docs != 0,
                completed: completed != 0,
                blocked: blocked != 0,
                block_reason,
            };
            (detail, completed_at_ms)
        };

        let before_snapshot = json!({
            "task": task_id,
            "step_id": detail.step_id,
            "path": detail.path,
            "title": detail.title,
            "success_criteria": detail.success_criteria,
            "tests": detail.tests,
            "blockers": detail.blockers,
            "criteria_confirmed": detail.criteria_confirmed,
            "tests_confirmed": detail.tests_confirmed,
            "security_confirmed": detail.security_confirmed,
            "perf_confirmed": detail.perf_confirmed,
            "docs_confirmed": detail.docs_confirmed,
            "completed": detail.completed,
            "completed_at_ms": before_completed_at_ms,
            "blocked": detail.blocked,
            "block_reason": detail.block_reason
        });

        if let Some(title) = patch.title {
            tx.execute(
                "UPDATE steps SET title=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, title, now_ms],
            )?;
        }
        if let Some(items) = patch.success_criteria {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET criteria_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }
        if let Some(items) = patch.tests {
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET tests_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }
        if let Some(items) = patch.blockers {
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_defined",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        let (after_detail, after_completed_at_ms) = {
            let row = tx
                .query_row(
                    r#"
                    SELECT title, criteria_confirmed, tests_confirmed, security_confirmed,
                           perf_confirmed, docs_confirmed, completed, completed_at_ms, blocked, block_reason
                    FROM steps
                    WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                    "#,
                    params![workspace.as_str(), task_id, step_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<i64>>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, Option<String>>(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                title,
                criteria,
                tests,
                security,
                perf,
                docs,
                completed,
                completed_at_ms,
                blocked,
                block_reason,
            )) = row
            else {
                return Err(StoreError::StepNotFound);
            };
            let detail = StepDetail {
                step_id: step_id.clone(),
                path: path.clone(),
                title,
                success_criteria: step_items_list_tx(
                    &tx,
                    workspace.as_str(),
                    &step_id,
                    "step_criteria",
                )?,
                tests: step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_tests")?,
                blockers: step_items_list_tx(&tx, workspace.as_str(), &step_id, "step_blockers")?,
                criteria_confirmed: criteria != 0,
                tests_confirmed: tests != 0,
                security_confirmed: security != 0,
                perf_confirmed: perf != 0,
                docs_confirmed: docs != 0,
                completed: completed != 0,
                blocked: blocked != 0,
                block_reason,
            };
            (detail, completed_at_ms)
        };

        let after_snapshot = json!({
            "task": task_id,
            "step_id": after_detail.step_id,
            "path": after_detail.path,
            "title": after_detail.title,
            "success_criteria": after_detail.success_criteria,
            "tests": after_detail.tests,
            "blockers": after_detail.blockers,
            "criteria_confirmed": after_detail.criteria_confirmed,
            "tests_confirmed": after_detail.tests_confirmed,
            "security_confirmed": after_detail.security_confirmed,
            "perf_confirmed": after_detail.perf_confirmed,
            "docs_confirmed": after_detail.docs_confirmed,
            "completed": after_detail.completed,
            "completed_at_ms": after_completed_at_ms,
            "blocked": after_detail.blocked,
            "block_reason": after_detail.block_reason
        });

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "step_patch",
                &event_payload_json,
                Some(&before_snapshot.to_string()),
                Some(&after_snapshot.to_string()),
                true,
                now_ms,
            )?;
        }

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_progress(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        completed: bool,
        force: bool,
        record_undo: bool,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let row = tx
            .query_row(
                "SELECT completed, completed_at_ms, criteria_confirmed, tests_confirmed, security_confirmed, perf_confirmed, docs_confirmed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;

        let Some((already_completed, completed_at_ms, criteria, tests, security, perf, docs)) = row
        else {
            return Err(StoreError::StepNotFound);
        };

        let before_snapshot = json!({
            "task": task_id,
            "step_id": step_id,
            "path": path.to_string(),
            "completed": already_completed != 0,
            "completed_at_ms": completed_at_ms
        });

        if completed {
            if already_completed != 0 {
                return Err(StoreError::InvalidInput("step already completed"));
            }
            let require_security =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "security")?;
            let require_perf =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "perf")?;
            let require_docs =
                checkpoint_required_tx(&tx, workspace.as_str(), "step", &step_id, "docs")?;

            if !force
                && (criteria == 0
                    || tests == 0
                    || (require_security && security == 0)
                    || (require_perf && perf == 0)
                    || (require_docs && docs == 0))
            {
                return Err(StoreError::CheckpointsNotConfirmed {
                    criteria: criteria == 0,
                    tests: tests == 0,
                    security: require_security && security == 0,
                    perf: require_perf && perf == 0,
                    docs: require_docs && docs == 0,
                });
            }
            tx.execute(
                "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        } else {
            tx.execute(
                "UPDATE steps SET completed=0, completed_at_ms=NULL, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
        }

        let after_snapshot = json!({
            "task": task_id,
            "step_id": step_id,
            "path": path.to_string(),
            "completed": completed,
            "completed_at_ms": if completed { Some(now_ms) } else { None }
        });

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let (event_type, event_payload_json) = if completed {
            ("step_done", build_step_done_payload(task_id, &step_ref))
        } else {
            (
                "step_reopened",
                build_step_reopened_payload(task_id, &step_ref, force),
            )
        };
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            event_type,
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "step_progress",
                &event_payload_json,
                Some(&before_snapshot.to_string()),
                Some(&after_snapshot.to_string()),
                true,
                now_ms,
            )?;
        }

        let (snapshot_title, snapshot_completed) =
            step_snapshot_tx(&tx, workspace.as_str(), task_id, &step_ref.step_id)?;
        let graph_touched = Self::project_task_graph_step_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &step_ref,
            &snapshot_title,
            snapshot_completed,
            now_ms,
        )?;
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_block_set(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        blocked: bool,
        reason: Option<String>,
        record_undo: bool,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let row = tx
            .query_row(
                "SELECT blocked, block_reason FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let Some((before_blocked, before_reason)) = row else {
            return Err(StoreError::StepNotFound);
        };

        let before_snapshot = json!({
            "task": task_id,
            "step_id": step_id,
            "path": path.to_string(),
            "blocked": before_blocked != 0,
            "block_reason": before_reason
        });

        let payload_reason = reason.clone();
        tx.execute(
            "UPDATE steps SET blocked=?4, block_reason=?5, updated_at_ms=?6 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![
                workspace.as_str(),
                task_id,
                step_id,
                if blocked { 1i64 } else { 0i64 },
                if blocked { reason.clone() } else { None::<String> },
                now_ms
            ],
        )?;

        let after_snapshot = json!({
            "task": task_id,
            "step_id": step_id,
            "path": path.to_string(),
            "blocked": blocked,
            "block_reason": if blocked { reason.clone() } else { None }
        });

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json =
            build_step_block_payload(task_id, &step_ref, blocked, payload_reason.as_deref());
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            if blocked {
                "step_blocked"
            } else {
                "step_unblocked"
            },
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "step_block_set",
                &event_payload_json,
                Some(&before_snapshot.to_string()),
                Some(&after_snapshot.to_string()),
                true,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn evidence_capture(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        artifacts: Vec<EvidenceArtifactInput>,
        checks: Vec<String>,
        attachments: Vec<String>,
    ) -> Result<EvidenceCaptureResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let (entity_kind, entity_id, path, revision, reasoning_kind) = if step_id.is_some()
            || path.is_some()
        {
            let task_revision =
                bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
            let (step_id, path) =
                resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
            (
                "step".to_string(),
                step_id,
                Some(path),
                task_revision,
                TaskKind::Task,
            )
        } else if task_id.starts_with("PLAN-") {
            let revision =
                bump_plan_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
            (
                "plan".to_string(),
                task_id.to_string(),
                None,
                revision,
                TaskKind::Plan,
            )
        } else if task_id.starts_with("TASK-") {
            let revision =
                bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
            (
                "task".to_string(),
                task_id.to_string(),
                None,
                revision,
                TaskKind::Task,
            )
        } else {
            return Err(StoreError::InvalidInput(
                "task must start with PLAN- or TASK-",
            ));
        };

        if !artifacts.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_artifacts WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, artifact) in artifacts.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    r#"
                    INSERT INTO evidence_artifacts(
                        workspace, entity_kind, entity_id, ordinal, kind,
                        command, stdout, stderr, exit_code, diff, content, url, external_uri, meta_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                    "#,
                    params![
                        workspace.as_str(),
                        entity_kind,
                        entity_id,
                        ordinal,
                        artifact.kind,
                        artifact.command,
                        artifact.stdout,
                        artifact.stderr,
                        artifact.exit_code,
                        artifact.diff,
                        artifact.content,
                        artifact.url,
                        artifact.external_uri,
                        artifact.meta_json
                    ],
                )?;
            }
        }

        if !checks.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_checks WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, check) in checks.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    "INSERT INTO evidence_checks(workspace, entity_kind, entity_id, ordinal, check_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![workspace.as_str(), entity_kind, entity_id, ordinal, check],
                )?;
            }
        }

        if !attachments.is_empty() {
            let base_ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), -1) FROM evidence_attachments WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3",
                params![workspace.as_str(), entity_kind, entity_id],
                |row| row.get(0),
            )?;
            for (idx, attachment) in attachments.iter().enumerate() {
                let ordinal = base_ordinal + idx as i64 + 1;
                tx.execute(
                    "INSERT INTO evidence_attachments(workspace, entity_kind, entity_id, ordinal, attachment) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![workspace.as_str(), entity_kind, entity_id, ordinal, attachment],
                )?;
            }
        }

        let event_payload_json = build_evidence_captured_payload(
            task_id,
            &entity_kind,
            &entity_id,
            path.as_deref(),
            artifacts.len(),
            checks.len(),
            attachments.len(),
        );
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            path.clone(),
            "evidence_captured",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, reasoning_kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok(EvidenceCaptureResult {
            revision,
            step: path.map(|p| StepRef {
                step_id: entity_id,
                path: p,
            }),
            event,
        })
    }

    pub fn step_delete(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        record_undo: bool,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let step_ids = collect_step_subtree_ids_tx(&tx, workspace.as_str(), task_id, &step_id)?;

        for step_id in step_ids.iter() {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM step_notes WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
                params![workspace.as_str(), step_id],
            )?;

            let node_ids = {
                let mut stmt = tx.prepare(
                    "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                )?;
                let rows = stmt
                    .query_map(params![workspace.as_str(), task_id, step_id], |row| {
                        row.get::<_, String>(0)
                    })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for node_id in node_ids {
                tx.execute(
                    "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
                    params![workspace.as_str(), node_id],
                )?;
            }
            tx.execute(
                "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                params![workspace.as_str(), task_id, step_id],
            )?;
        }

        for step_id in step_ids.iter() {
            tx.execute(
                "DELETE FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_deleted_payload(task_id, &step_ref);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_deleted",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(task_id),
                Some(path.clone()),
                "step_delete",
                &event_payload_json,
                None,
                None,
                false,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn task_root_delete(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        record_undo: bool,
    ) -> Result<(TaskKind, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let kind = if id.starts_with("PLAN-") {
            TaskKind::Plan
        } else if id.starts_with("TASK-") {
            TaskKind::Task
        } else {
            return Err(StoreError::InvalidInput(
                "task must start with PLAN- or TASK-",
            ));
        };

        if matches!(kind, TaskKind::Plan) {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::UnknownId);
            }
            let task_ids = {
                let mut stmt =
                    tx.prepare("SELECT id FROM tasks WHERE workspace=?1 AND parent_plan_id=?2")?;
                let rows = stmt.query_map(params![workspace.as_str(), id], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for task_id in task_ids.iter() {
                delete_task_rows_tx(&tx, workspace.as_str(), task_id)?;
                let event_payload_json = build_task_deleted_payload(task_id, TaskKind::Task);
                let task_event = insert_event_tx(
                    &tx,
                    workspace.as_str(),
                    now_ms,
                    Some(task_id.to_string()),
                    None,
                    "task_deleted",
                    &event_payload_json,
                )?;
                let reasoning_ref =
                    ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
                let _ = ingest_task_event_tx(
                    &tx,
                    workspace.as_str(),
                    &reasoning_ref.branch,
                    &reasoning_ref.trace_doc,
                    &task_event,
                )?;
            }

            tx.execute(
                "DELETE FROM plan_checklist WHERE workspace=?1 AND plan_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='plan' AND entity_id=?2",
                params![workspace.as_str(), id],
            )?;
            tx.execute(
                "DELETE FROM plans WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), id],
            )?;
        } else {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::UnknownId);
            }
            delete_task_rows_tx(&tx, workspace.as_str(), id)?;
        }

        let event_payload_json = build_task_deleted_payload(id, kind);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            "task_deleted",
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                workspace.as_str(),
                Some(id),
                None,
                "task_delete",
                &event_payload_json,
                None,
                None,
                false,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok((kind, event))
    }

    pub fn task_steps_summary(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
    ) -> Result<TaskStepSummary, StoreError> {
        let tx = self.conn.transaction()?;

        let exists = tx
            .query_row(
                "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), task_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(StoreError::UnknownId);
        }

        let total_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let completed_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=1",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let open_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_criteria: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND criteria_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_tests: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND tests_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_security: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.security_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='security'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='security'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_perf: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.perf_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='perf'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='perf'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_docs: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM steps s
            WHERE s.workspace=?1
              AND s.task_id=?2
              AND s.completed=0
              AND s.docs_confirmed=0
              AND (
                EXISTS (
                  SELECT 1 FROM checkpoint_notes n
                  WHERE n.workspace=s.workspace AND n.entity_kind='step' AND n.entity_id=s.step_id AND n.checkpoint='docs'
                )
                OR EXISTS (
                  SELECT 1 FROM checkpoint_evidence e
                  WHERE e.workspace=s.workspace AND e.entity_kind='step' AND e.entity_id=s.step_id AND e.checkpoint='docs'
                )
              )
            "#,
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let first_open = tx
            .query_row(
                r#"
                SELECT step_id, title, completed, criteria_confirmed, tests_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND completed=0
                ORDER BY created_at_ms ASC
                LIMIT 1
                "#,
                params![workspace.as_str(), task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?
            .map(
                |(step_id, title, completed, criteria, tests, security, perf, docs)| {
                    let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id, &step_id)
                        .unwrap_or_else(|_| "s:?".to_string());
                    StepStatus {
                        step_id,
                        path,
                        title,
                        completed: completed != 0,
                        criteria_confirmed: criteria != 0,
                        tests_confirmed: tests != 0,
                        security_confirmed: security != 0,
                        perf_confirmed: perf != 0,
                        docs_confirmed: docs != 0,
                    }
                },
            );

        tx.commit()?;
        Ok(TaskStepSummary {
            total_steps,
            completed_steps,
            open_steps,
            missing_criteria,
            missing_tests,
            missing_security,
            missing_perf,
            missing_docs,
            first_open,
        })
    }

    pub fn task_open_blockers(
        &self,
        workspace: &WorkspaceId,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT b.text
            FROM step_blockers b
            JOIN steps s
              ON s.workspace = b.workspace AND s.step_id = b.step_id
            WHERE s.workspace = ?1 AND s.task_id = ?2 AND s.completed = 0
            ORDER BY s.created_at_ms ASC, b.ordinal ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), task_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn task_items_list(
        &mut self,
        workspace: &WorkspaceId,
        entity_kind: &str,
        entity_id: &str,
        field: &str,
    ) -> Result<Vec<String>, StoreError> {
        let tx = self.conn.transaction()?;
        let items = task_items_list_tx(&tx, workspace.as_str(), entity_kind, entity_id, field)?;
        tx.commit()?;
        Ok(items)
    }

    pub fn list_plans(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<PlanRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, title, contract, contract_json, description, context,
                   status, status_manual, priority, plan_doc, plan_current,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM plans
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
            |row| {
                Ok(PlanRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    title: row.get(2)?,
                    contract: row.get(3)?,
                    contract_json: row.get(4)?,
                    description: row.get(5)?,
                    context: row.get(6)?,
                    status: row.get(7)?,
                    status_manual: row.get::<_, i64>(8)? != 0,
                    priority: row.get(9)?,
                    plan_doc: row.get(10)?,
                    plan_current: row.get(11)?,
                    criteria_confirmed: row.get::<_, i64>(12)? != 0,
                    tests_confirmed: row.get::<_, i64>(13)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(14)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(15)? != 0,
                    security_confirmed: row.get::<_, i64>(16)? != 0,
                    perf_confirmed: row.get::<_, i64>(17)? != 0,
                    docs_confirmed: row.get::<_, i64>(18)? != 0,
                    created_at_ms: row.get(19)?,
                    updated_at_ms: row.get(20)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn count_plans(&self, workspace: &WorkspaceId) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM plans WHERE workspace = ?1",
            params![workspace.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn list_tasks(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description,
                   status, status_manual, priority, blocked,
                   assignee, domain, phase, component, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM tasks
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
            |row| {
                Ok(TaskRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_plan_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    status: row.get(5)?,
                    status_manual: row.get::<_, i64>(6)? != 0,
                    priority: row.get(7)?,
                    blocked: row.get::<_, i64>(8)? != 0,
                    assignee: row.get(9)?,
                    domain: row.get(10)?,
                    phase: row.get(11)?,
                    component: row.get(12)?,
                    context: row.get(13)?,
                    criteria_confirmed: row.get::<_, i64>(14)? != 0,
                    tests_confirmed: row.get::<_, i64>(15)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(16)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    security_confirmed: row.get::<_, i64>(18)? != 0,
                    perf_confirmed: row.get::<_, i64>(19)? != 0,
                    docs_confirmed: row.get::<_, i64>(20)? != 0,
                    created_at_ms: row.get(21)?,
                    updated_at_ms: row.get(22)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_tasks_for_plan(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description,
                   status, status_manual, priority, blocked,
                   assignee, domain, phase, component, context,
                   criteria_confirmed, tests_confirmed, criteria_auto_confirmed, tests_auto_confirmed,
                   security_confirmed, perf_confirmed, docs_confirmed,
                   created_at_ms, updated_at_ms
            FROM tasks
            WHERE workspace = ?1 AND parent_plan_id = ?2
            ORDER BY id ASC
            LIMIT ?3 OFFSET ?4
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), plan_id, limit as i64, offset as i64],
            |row| {
                Ok(TaskRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_plan_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    status: row.get(5)?,
                    status_manual: row.get::<_, i64>(6)? != 0,
                    priority: row.get(7)?,
                    blocked: row.get::<_, i64>(8)? != 0,
                    assignee: row.get(9)?,
                    domain: row.get(10)?,
                    phase: row.get(11)?,
                    component: row.get(12)?,
                    context: row.get(13)?,
                    criteria_confirmed: row.get::<_, i64>(14)? != 0,
                    tests_confirmed: row.get::<_, i64>(15)? != 0,
                    criteria_auto_confirmed: row.get::<_, i64>(16)? != 0,
                    tests_auto_confirmed: row.get::<_, i64>(17)? != 0,
                    security_confirmed: row.get::<_, i64>(18)? != 0,
                    perf_confirmed: row.get::<_, i64>(19)? != 0,
                    docs_confirmed: row.get::<_, i64>(20)? != 0,
                    created_at_ms: row.get(21)?,
                    updated_at_ms: row.get(22)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_events_for_task(
        &self,
        workspace: &WorkspaceId,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, type, payload_json
            FROM events
            WHERE workspace = ?1 AND task_id = ?2
            ORDER BY seq DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), task_id, limit as i64],
            |row| {
                Ok(EventRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    task_id: row.get(2)?,
                    path: row.get(3)?,
                    event_type: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_task_steps(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        parent_path: Option<&StepPath>,
        limit: usize,
    ) -> Result<Vec<StepListRow>, StoreError> {
        let tx = self.conn.transaction()?;
        let subtree_ids = if let Some(path) = parent_path {
            let step_id = resolve_step_id_tx(&tx, workspace.as_str(), task_id, path)?;
            let ids = collect_step_subtree_ids_tx(&tx, workspace.as_str(), task_id, &step_id)?;
            Some(ids.into_iter().collect::<HashSet<_>>())
        } else {
            None
        };

        let raw_rows = {
            let mut stmt = tx.prepare(
                r#"
                SELECT step_id, title, completed, criteria_confirmed, tests_confirmed,
                       security_confirmed, perf_confirmed, docs_confirmed, blocked, block_reason
                FROM steps
                WHERE workspace=?1 AND task_id=?2
                "#,
            )?;
            let rows = stmt.query_map(params![workspace.as_str(), task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut steps = Vec::new();
        for row in raw_rows {
            let (
                step_id,
                title,
                completed,
                criteria,
                tests,
                security,
                perf,
                docs,
                blocked,
                block_reason,
            ) = row;
            if let Some(ref ids) = subtree_ids {
                if !ids.contains(&step_id) {
                    continue;
                }
            }
            let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id, &step_id)?;
            steps.push(StepListRow {
                step_id,
                path,
                title,
                completed: completed != 0,
                criteria_confirmed: criteria != 0,
                tests_confirmed: tests != 0,
                security_confirmed: security != 0,
                perf_confirmed: perf != 0,
                docs_confirmed: docs != 0,
                blocked: blocked != 0,
                block_reason,
            });
        }

        steps.sort_by(|a, b| a.path.cmp(&b.path));
        if limit > 0 && steps.len() > limit {
            steps.truncate(limit);
        }

        tx.commit()?;
        Ok(steps)
    }

    pub fn count_tasks(&self, workspace: &WorkspaceId) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE workspace = ?1",
            params![workspace.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn list_events(
        &self,
        workspace: &WorkspaceId,
        since_event_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        let since_seq = match since_event_id {
            None => 0i64,
            Some(event_id) => parse_event_id(event_id).ok_or(StoreError::InvalidInput(
                "since must be like evt_<16-digit-seq>",
            ))?,
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, type, payload_json
            FROM events
            WHERE workspace = ?1 AND seq > ?2
            ORDER BY seq ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), since_seq, limit as i64],
            |row| {
                Ok(EventRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    task_id: row.get(2)?,
                    path: row.get(3)?,
                    event_type: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn ops_history_list(
        &self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OpsHistoryRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, intent, payload_json, before_json, after_json, undoable, undone
            FROM ops_history
            WHERE workspace=?1 AND (?2 IS NULL OR task_id=?2)
            ORDER BY seq DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), task_id, limit as i64],
            |row| {
                Ok(OpsHistoryRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    task_id: row.get(2)?,
                    path: row.get(3)?,
                    intent: row.get(4)?,
                    payload_json: row.get(5)?,
                    before_json: row.get(6)?,
                    after_json: row.get(7)?,
                    undoable: row.get::<_, i64>(8)? != 0,
                    undone: row.get::<_, i64>(9)? != 0,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn ops_history_undo(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        self.ops_history_apply(workspace, task_id, true)
    }

    pub fn ops_history_redo(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        self.ops_history_apply(workspace, task_id, false)
    }

    fn ops_history_apply(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
        undo: bool,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let undone_flag = if undo { 0i64 } else { 1i64 };
        let row = {
            let mut stmt = tx.prepare(
                r#"
                SELECT seq, ts_ms, task_id, path, intent, payload_json, before_json, after_json, undoable, undone
                FROM ops_history
                WHERE workspace=?1 AND undoable=1 AND undone=?2 AND (?3 IS NULL OR task_id=?3)
                ORDER BY seq DESC
                LIMIT 1
                "#,
            )?;
            stmt.query_row(
                params![workspace.as_str(), undone_flag, task_id],
                |row| {
                    Ok(OpsHistoryRow {
                        seq: row.get(0)?,
                        ts_ms: row.get(1)?,
                        task_id: row.get(2)?,
                        path: row.get(3)?,
                        intent: row.get(4)?,
                        payload_json: row.get(5)?,
                        before_json: row.get(6)?,
                        after_json: row.get(7)?,
                        undoable: row.get::<_, i64>(8)? != 0,
                        undone: row.get::<_, i64>(9)? != 0,
                    })
                },
            )
            .optional()?
        };
        let Some(row) = row else {
            return Err(StoreError::InvalidInput("no undoable operations"));
        };

        let snapshot_json = if undo {
            row.before_json.as_deref()
        } else {
            row.after_json.as_deref()
        }
        .ok_or(StoreError::InvalidInput("snapshot missing"))?;
        let snapshot: JsonValue =
            serde_json::from_str(snapshot_json).map_err(|_| StoreError::InvalidInput("snapshot invalid"))?;

        let target = match row.intent.as_str() {
            "task_detail_patch" => apply_task_detail_snapshot_tx(&tx, workspace, &snapshot, now_ms)?,
            "step_patch" => apply_step_patch_snapshot_tx(&tx, workspace, &snapshot, now_ms)?,
            "step_progress" => apply_step_progress_snapshot_tx(&tx, workspace, &snapshot, now_ms)?,
            "step_block_set" => apply_step_block_snapshot_tx(&tx, workspace, &snapshot, now_ms)?,
            "task_node_patch" => apply_task_node_snapshot_tx(&tx, workspace, &snapshot, now_ms)?,
            _ => return Err(StoreError::InvalidInput("undo not supported for intent")),
        };

        tx.execute(
            "UPDATE ops_history SET undone=?3 WHERE workspace=?1 AND seq=?2",
            params![workspace.as_str(), row.seq, if undo { 1i64 } else { 0i64 }],
        )?;

        let event_payload_json = build_undo_redo_payload(
            row.seq,
            row.intent.as_str(),
            row.task_id.as_deref(),
            row.path.as_deref(),
            undo,
        );
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            row.task_id.clone(),
            row.path.clone(),
            if undo { "undo_applied" } else { "redo_applied" },
            &event_payload_json,
        )?;

        if let Some(task_id) = row.task_id.as_deref() {
            let kind = parse_plan_or_task_kind(task_id)?;
            let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, task_id, kind, now_ms)?;
            let _ = ingest_task_event_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.trace_doc,
                &event,
            )?;

            match target {
                OpsHistoryTarget::Task { title } => {
                    if let Some(title) = title {
                        let touched = Self::project_task_graph_task_node_tx(
                            &tx,
                            workspace.as_str(),
                            &reasoning_ref,
                            &event,
                            task_id,
                            &title,
                            now_ms,
                        )?;
                        if touched {
                            touch_document_tx(
                                &tx,
                                workspace.as_str(),
                                &reasoning_ref.branch,
                                &reasoning_ref.graph_doc,
                                now_ms,
                            )?;
                        }
                    }
                }
                OpsHistoryTarget::Step { step } => {
                    let (snapshot_title, snapshot_completed) =
                        step_snapshot_tx(&tx, workspace.as_str(), task_id, &step.step_id)?;
                    let graph_touched = Self::project_task_graph_step_node_tx(
                        &tx,
                        workspace.as_str(),
                        &reasoning_ref,
                        &event,
                        task_id,
                        &step,
                        &snapshot_title,
                        snapshot_completed,
                        now_ms,
                    )?;
                    if graph_touched {
                        touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &reasoning_ref.branch,
                            &reasoning_ref.graph_doc,
                            now_ms,
                        )?;
                    }
                }
                OpsHistoryTarget::TaskNode => {}
            }
        }

        tx.commit()?;
        Ok((row, event))
    }

    pub fn workspace_exists(&self, workspace: &WorkspaceId) -> Result<bool, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM workspaces WHERE workspace=?1",
                params![workspace.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn workspace_last_event_head(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<(i64, i64)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, ts_ms FROM events WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
                params![workspace.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?)
    }

    pub fn workspace_last_doc_entry_head(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<(i64, i64, String, String, String)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, ts_ms, branch, doc, kind FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
                params![workspace.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?)
    }
}

fn snapshot_required_str(snapshot: &JsonValue, field: &str) -> Result<String, StoreError> {
    snapshot
        .get(field)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| StoreError::InvalidInput("snapshot missing string field"))
}

fn snapshot_required_bool(snapshot: &JsonValue, field: &str) -> Result<bool, StoreError> {
    snapshot
        .get(field)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| StoreError::InvalidInput("snapshot missing boolean field"))
}

fn snapshot_optional_string(snapshot: &JsonValue, field: &str) -> Result<Option<String>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        _ => Err(StoreError::InvalidInput("snapshot invalid string field")),
    }
}

fn snapshot_optional_i64(snapshot: &JsonValue, field: &str) -> Result<Option<i64>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .ok_or(StoreError::InvalidInput("snapshot invalid integer field"))
            .map(Some),
    }
}

fn snapshot_optional_json_string(
    snapshot: &JsonValue,
    field: &str,
) -> Result<Option<String>, StoreError> {
    match snapshot.get(field) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => Ok(Some(value.to_string())),
    }
}

fn snapshot_required_vec(snapshot: &JsonValue, field: &str) -> Result<Vec<String>, StoreError> {
    let Some(array) = snapshot.get(field).and_then(|v| v.as_array()) else {
        return Err(StoreError::InvalidInput("snapshot missing array field"));
    };
    let mut out = Vec::with_capacity(array.len());
    for value in array {
        let Some(text) = value.as_str() else {
            return Err(StoreError::InvalidInput("snapshot array must be strings"));
        };
        out.push(text.to_string());
    }
    Ok(out)
}

fn apply_task_detail_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let kind_raw = snapshot_required_str(snapshot, "kind")?;
    let kind = match kind_raw.as_str() {
        "plan" => TaskKind::Plan,
        "task" => TaskKind::Task,
        _ => return Err(StoreError::InvalidInput("snapshot kind invalid")),
    };
    let task_id = snapshot_required_str(snapshot, "task")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let description = snapshot_optional_string(snapshot, "description")?;
    let context = snapshot_optional_string(snapshot, "context")?;
    let priority = snapshot_required_str(snapshot, "priority")?;
    let contract = snapshot_optional_string(snapshot, "contract")?;
    let contract_json = snapshot_optional_json_string(snapshot, "contract_data")?;
    let domain = snapshot_optional_string(snapshot, "domain")?;
    let phase = snapshot_optional_string(snapshot, "phase")?;
    let component = snapshot_optional_string(snapshot, "component")?;
    let assignee = snapshot_optional_string(snapshot, "assignee")?;
    let tags = snapshot_required_vec(snapshot, "tags")?;
    let depends_on = snapshot_required_vec(snapshot, "depends_on")?;

    match kind {
        TaskKind::Plan => {
            bump_plan_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;
            let changed = tx.execute(
                r#"
                UPDATE plans
                SET title=?3, description=?4, context=?5, priority=?6, contract=?7, contract_json=?8, updated_at_ms=?9
                WHERE workspace=?1 AND id=?2
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    title,
                    description,
                    context,
                    priority,
                    contract,
                    contract_json,
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::UnknownId);
            }
        }
        TaskKind::Task => {
            bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;
            let changed = tx.execute(
                r#"
                UPDATE tasks
                SET title=?3, description=?4, context=?5, priority=?6,
                    domain=?7, phase=?8, component=?9, assignee=?10, updated_at_ms=?11
                WHERE workspace=?1 AND id=?2
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    title,
                    description,
                    context,
                    priority,
                    domain,
                    phase,
                    component,
                    assignee,
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::UnknownId);
            }
        }
    }

    task_items_replace_tx(
        tx,
        workspace.as_str(),
        kind.as_str(),
        &task_id,
        "tags",
        &tags,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        kind.as_str(),
        &task_id,
        "depends_on",
        &depends_on,
    )?;

    Ok(OpsHistoryTarget::Task {
        title: if matches!(kind, TaskKind::Task) {
            Some(title)
        } else {
            None
        },
    })
}

fn apply_step_patch_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let success_criteria = snapshot_required_vec(snapshot, "success_criteria")?;
    let tests = snapshot_required_vec(snapshot, "tests")?;
    let blockers = snapshot_required_vec(snapshot, "blockers")?;
    let criteria_confirmed = snapshot_required_bool(snapshot, "criteria_confirmed")?;
    let tests_confirmed = snapshot_required_bool(snapshot, "tests_confirmed")?;
    let security_confirmed = snapshot_required_bool(snapshot, "security_confirmed")?;
    let perf_confirmed = snapshot_required_bool(snapshot, "perf_confirmed")?;
    let docs_confirmed = snapshot_required_bool(snapshot, "docs_confirmed")?;
    let completed = snapshot_required_bool(snapshot, "completed")?;
    let completed_at_ms = snapshot_optional_i64(snapshot, "completed_at_ms")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let block_reason = snapshot_optional_string(snapshot, "block_reason")?;

    bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    if completed {
        if let Some(completed_at_ms) = completed_at_ms {
            let changed = tx.execute(
                r#"
                UPDATE steps
                SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                    perf_confirmed=?8, docs_confirmed=?9, completed=?10, completed_at_ms=?11,
                    blocked=?12, block_reason=?13, updated_at_ms=?14
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    if criteria_confirmed { 1i64 } else { 0i64 },
                    if tests_confirmed { 1i64 } else { 0i64 },
                    if security_confirmed { 1i64 } else { 0i64 },
                    if perf_confirmed { 1i64 } else { 0i64 },
                    if docs_confirmed { 1i64 } else { 0i64 },
                    1i64,
                    completed_at_ms,
                    if blocked { 1i64 } else { 0i64 },
                    if blocked { block_reason.clone() } else { None },
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::StepNotFound);
            }
        } else {
            let changed = tx.execute(
                r#"
                UPDATE steps
                SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                    perf_confirmed=?8, docs_confirmed=?9, completed=?10,
                    blocked=?11, block_reason=?12, updated_at_ms=?13
                WHERE workspace=?1 AND task_id=?2 AND step_id=?3
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    title,
                    if criteria_confirmed { 1i64 } else { 0i64 },
                    if tests_confirmed { 1i64 } else { 0i64 },
                    if security_confirmed { 1i64 } else { 0i64 },
                    if perf_confirmed { 1i64 } else { 0i64 },
                    if docs_confirmed { 1i64 } else { 0i64 },
                    1i64,
                    if blocked { 1i64 } else { 0i64 },
                    if blocked { block_reason.clone() } else { None },
                    now_ms
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::StepNotFound);
            }
        }
    } else {
        let changed = tx.execute(
            r#"
            UPDATE steps
            SET title=?4, criteria_confirmed=?5, tests_confirmed=?6, security_confirmed=?7,
                perf_confirmed=?8, docs_confirmed=?9, completed=?10, completed_at_ms=NULL,
                blocked=?11, block_reason=?12, updated_at_ms=?13
            WHERE workspace=?1 AND task_id=?2 AND step_id=?3
            "#,
            params![
                workspace.as_str(),
                task_id,
                step_id,
                title,
                if criteria_confirmed { 1i64 } else { 0i64 },
                if tests_confirmed { 1i64 } else { 0i64 },
                if security_confirmed { 1i64 } else { 0i64 },
                if perf_confirmed { 1i64 } else { 0i64 },
                if docs_confirmed { 1i64 } else { 0i64 },
                0i64,
                if blocked { 1i64 } else { 0i64 },
                if blocked { block_reason.clone() } else { None },
                now_ms
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    }

    tx.execute(
        "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in success_criteria.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }
    tx.execute(
        "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in tests.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }
    tx.execute(
        "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
        params![workspace.as_str(), step_id],
    )?;
    for (idx, text) in blockers.iter().enumerate() {
        tx.execute(
            "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
            params![workspace.as_str(), step_id, idx as i64, text],
        )?;
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}

fn apply_step_progress_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let completed = snapshot_required_bool(snapshot, "completed")?;
    let completed_at_ms = snapshot_optional_i64(snapshot, "completed_at_ms")?;

    bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    if completed {
        let ts = completed_at_ms.unwrap_or(now_ms);
        let changed = tx.execute(
            "UPDATE steps SET completed=1, completed_at_ms=?4, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, ts],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    } else {
        let changed = tx.execute(
            "UPDATE steps SET completed=0, completed_at_ms=NULL, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;
        if changed == 0 {
            return Err(StoreError::StepNotFound);
        }
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}

fn apply_step_block_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let step_id = snapshot_required_str(snapshot, "step_id")?;
    let path = snapshot_required_str(snapshot, "path")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let block_reason = snapshot_optional_string(snapshot, "block_reason")?;

    bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    let changed = tx.execute(
        "UPDATE steps SET blocked=?4, block_reason=?5, updated_at_ms=?6 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
        params![
            workspace.as_str(),
            task_id,
            step_id,
            if blocked { 1i64 } else { 0i64 },
            if blocked { block_reason.clone() } else { None },
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(StoreError::StepNotFound);
    }

    Ok(OpsHistoryTarget::Step {
        step: StepRef { step_id, path },
    })
}

fn apply_task_node_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    let task_id = snapshot_required_str(snapshot, "task")?;
    let node_id = snapshot_required_str(snapshot, "node_id")?;
    let title = snapshot_required_str(snapshot, "title")?;
    let status = snapshot_required_str(snapshot, "status")?;
    let status_manual = snapshot_required_bool(snapshot, "status_manual")?;
    let priority = snapshot_required_str(snapshot, "priority")?;
    let blocked = snapshot_required_bool(snapshot, "blocked")?;
    let description = snapshot_optional_string(snapshot, "description")?;
    let context = snapshot_optional_string(snapshot, "context")?;
    let blockers = snapshot_required_vec(snapshot, "blockers")?;
    let dependencies = snapshot_required_vec(snapshot, "dependencies")?;
    let next_steps = snapshot_required_vec(snapshot, "next_steps")?;
    let problems = snapshot_required_vec(snapshot, "problems")?;
    let risks = snapshot_required_vec(snapshot, "risks")?;
    let success_criteria = snapshot_required_vec(snapshot, "success_criteria")?;

    bump_task_revision_tx(tx, workspace.as_str(), &task_id, None, now_ms)?;

    let changed = tx.execute(
        r#"
        UPDATE task_nodes
        SET title=?4, status=?5, status_manual=?6, priority=?7, blocked=?8,
            description=?9, context=?10, updated_at_ms=?11
        WHERE workspace=?1 AND task_id=?2 AND node_id=?3
        "#,
        params![
            workspace.as_str(),
            task_id,
            node_id,
            title,
            status,
            if status_manual { 1i64 } else { 0i64 },
            priority,
            if blocked { 1i64 } else { 0i64 },
            description,
            context,
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(StoreError::UnknownId);
    }

    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "blockers",
        &blockers,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "dependencies",
        &dependencies,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "next_steps",
        &next_steps,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "problems",
        &problems,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "risks",
        &risks,
    )?;
    task_items_replace_tx(
        tx,
        workspace.as_str(),
        "task_node",
        &node_id,
        "success_criteria",
        &success_criteria,
    )?;

    Ok(OpsHistoryTarget::TaskNode)
}

fn now_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

fn ensure_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    kind: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO documents(workspace, branch, doc, kind, created_at_ms, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(workspace, branch, doc) DO NOTHING
        "#,
        params![workspace, branch, doc, kind, now_ms, now_ms],
    )?;
    Ok(())
}

fn touch_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE documents SET updated_at_ms=?4 WHERE workspace=?1 AND branch=?2 AND doc=?3",
        params![workspace, branch, doc, now_ms],
    )?;
    Ok(())
}

fn ensure_reasoning_ref_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    id: &str,
    kind: TaskKind,
    now_ms: i64,
) -> Result<ReasoningRefRow, StoreError> {
    let reference = ReasoningRef::for_entity(kind, id);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace.as_str(),
            id,
            kind.as_str(),
            &reference.branch,
            &reference.notes_doc,
            &reference.graph_doc,
            &reference.trace_doc,
            now_ms
        ],
    )?;
    Ok(ReasoningRefRow {
        branch: reference.branch,
        notes_doc: reference.notes_doc,
        graph_doc: reference.graph_doc,
        trace_doc: reference.trace_doc,
    })
}

fn ingest_task_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    event: &EventRow,
) -> Result<bool, StoreError> {
    ensure_document_tx(
        tx,
        workspace,
        branch,
        doc,
        DocumentKind::Trace.as_str(),
        event.ts_ms,
    )?;

    let event_id = event.event_id();
    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, source_event_id, event_type, task_id, path, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            workspace,
            branch,
            doc,
            event.ts_ms,
            DocEntryKind::Event.as_str(),
            event_id,
            &event.event_type,
            event.task_id.as_deref(),
            event.path.as_deref(),
            &event.payload_json
        ],
    )?;

    if inserted > 0 {
        touch_document_tx(tx, workspace, branch, doc, event.ts_ms)?;
    }

    Ok(inserted > 0)
}

fn doc_entries_head_seq_tx(
    tx: &Transaction<'_>,
    workspace: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT seq FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
            params![workspace],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn doc_head_seq_for_sources_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    doc: &str,
    sources: &[BranchSource],
) -> Result<Option<i64>, StoreError> {
    let mut sql = String::from(
        "SELECT MAX(seq) FROM doc_entries WHERE workspace=?1 AND doc=?2 AND (",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));

    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(branch=?");
        params.push(SqlValue::Text(source.branch.clone()));
        if let Some(cutoff) = source.cutoff_seq {
            sql.push_str(" AND seq <= ?");
            params.push(SqlValue::Integer(cutoff));
        }
        sql.push(')');
    }
    sql.push(')');

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;
    if let Some(row) = rows.next()? {
        Ok(row.get::<_, Option<i64>>(0)?)
    } else {
        Ok(None)
    }
}

fn doc_entry_visible_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    seq: i64,
) -> Result<bool, StoreError> {
    if seq <= 0 {
        return Ok(false);
    }
    if !branch_exists_tx(tx, workspace, branch)? {
        return Err(StoreError::UnknownBranch);
    }

    let row = tx
        .query_row(
            "SELECT branch FROM doc_entries WHERE workspace=?1 AND doc=?2 AND seq=?3",
            params![workspace, doc, seq],
            |row| Ok(row.get::<_, String>(0)?),
        )
        .optional()?;
    let Some(entry_branch) = row else {
        return Ok(false);
    };

    let sources = branch_sources_tx(tx, workspace, branch)?;
    for source in sources {
        if source.branch == entry_branch {
            if let Some(cutoff) = source.cutoff_seq {
                return Ok(seq <= cutoff);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn branch_exists_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<bool, StoreError> {
    if tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    Ok(false)
}

#[derive(Clone, Debug)]
struct BranchSource {
    branch: String,
    cutoff_seq: Option<i64>,
}

fn branch_sources_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Vec<BranchSource>, StoreError> {
    use std::collections::HashSet;

    const MAX_DEPTH: usize = 32;

    let mut sources = Vec::new();
    sources.push(BranchSource {
        branch: branch.to_string(),
        cutoff_seq: None,
    });

    let mut seen = HashSet::new();
    seen.insert(branch.to_string());

    let mut current = branch.to_string();
    let mut inherited_cutoff: Option<i64> = None;

    for depth in 0..MAX_DEPTH {
        let row = tx
            .query_row(
                "SELECT base_branch, base_seq FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace, current],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((base_branch, base_seq)) = row else {
            break;
        };

        if base_branch == current {
            break;
        }

        if seen.contains(&base_branch) {
            return Err(StoreError::BranchCycle);
        }

        let effective = match inherited_cutoff {
            None => base_seq,
            Some(prev) => std::cmp::min(prev, base_seq),
        };

        sources.push(BranchSource {
            branch: base_branch.clone(),
            cutoff_seq: Some(effective),
        });

        seen.insert(base_branch.clone());
        current = base_branch;
        inherited_cutoff = Some(effective);

        if depth == MAX_DEPTH - 1 {
            return Err(StoreError::BranchDepthExceeded);
        }
    }

    Ok(sources)
}

fn doc_diff_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    to_branch: &str,
    doc: &str,
    before_seq: i64,
    limit: i64,
) -> Result<DocSlice, StoreError> {
    let from_sources = branch_sources_tx(tx, workspace, from_branch)?;
    let to_sources = branch_sources_tx(tx, workspace, to_branch)?;

    let mut sql = String::from(
        "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
         FROM doc_entries \
         WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));

    append_sources_clause(&mut sql, &mut params, &to_sources);
    sql.push_str(" AND NOT ");
    append_sources_clause(&mut sql, &mut params, &from_sources);
    sql.push_str(" ORDER BY seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit + 1));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;

    let mut entries_desc = Vec::new();
    while let Some(row) = rows.next()? {
        let kind_str: String = row.get(3)?;
        let kind = match kind_str.as_str() {
            "note" => DocEntryKind::Note,
            "event" => DocEntryKind::Event,
            _ => DocEntryKind::Event,
        };
        entries_desc.push(DocEntryRow {
            seq: row.get(0)?,
            ts_ms: row.get(1)?,
            branch: row.get(2)?,
            doc: doc.to_string(),
            kind,
            title: row.get(4)?,
            format: row.get(5)?,
            meta_json: row.get(6)?,
            content: row.get(7)?,
            source_event_id: row.get(8)?,
            event_type: row.get(9)?,
            task_id: row.get(10)?,
            path: row.get(11)?,
            payload_json: row.get(12)?,
        });
    }

    let has_more = entries_desc.len() as i64 > limit;
    if has_more {
        entries_desc.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        entries_desc.last().map(|e| e.seq)
    } else {
        None
    };

    entries_desc.reverse();

    Ok(DocSlice {
        entries: entries_desc,
        next_cursor,
        has_more,
    })
}

fn append_sources_clause(sql: &mut String, params: &mut Vec<SqlValue>, sources: &[BranchSource]) {
    sql.push('(');
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(branch=?");
        params.push(SqlValue::Text(source.branch.clone()));
        if let Some(cutoff) = source.cutoff_seq {
            sql.push_str(" AND seq <= ?");
            params.push(SqlValue::Integer(cutoff));
        }
        sql.push(')');
    }
    sql.push(')');
}

fn validate_graph_node_id(value: &str) -> Result<(), StoreError> {
    GraphNodeId::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

fn validate_graph_type(value: &str) -> Result<(), StoreError> {
    GraphType::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

fn validate_graph_rel(value: &str) -> Result<(), StoreError> {
    GraphRel::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

fn validate_conflict_id(value: &str) -> Result<(), StoreError> {
    ConflictId::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

fn normalize_tags(tags: &[String]) -> Result<Vec<String>, StoreError> {
    core_normalize_tags(tags).map_err(|err| match err {
        GraphTagError::ContainsPipe => StoreError::InvalidInput(err.message()),
        GraphTagError::ContainsControl => StoreError::InvalidInput(err.message()),
    })
}

fn encode_tags(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    Some(format!("\n{}\n", tags.join("\n")))
}

fn decode_tags(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split('\n')
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect()
}

fn build_graph_op_event(op: &GraphOp) -> (&'static str, String) {
    fn push_opt_str(out: &mut String, key: &str, value: Option<&str>) {
        let Some(value) = value else {
            return;
        };
        out.push_str(",\"");
        out.push_str(key);
        out.push_str("\":\"");
        out.push_str(&json_escape(value));
        out.push('"');
    }

    fn push_opt_meta(out: &mut String, meta_json: Option<&str>) {
        let Some(meta_json) = meta_json else {
            return;
        };
        let trimmed = meta_json.trim();
        if looks_like_json_object(trimmed) {
            out.push_str(",\"meta\":");
            out.push_str(trimmed);
        } else {
            out.push_str(",\"meta_raw\":\"");
            out.push_str(&json_escape(trimmed));
            out.push('"');
        }
    }

    fn push_tags(out: &mut String, tags: &[String]) {
        if tags.is_empty() {
            return;
        }
        out.push_str(",\"tags\":[");
        for (i, tag) in tags.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&json_escape(tag));
            out.push('"');
        }
        out.push(']');
    }

    match op {
        GraphOp::NodeUpsert(upsert) => {
            let mut out = String::new();
            out.push_str("{\"op\":\"node_upsert\",\"id\":\"");
            out.push_str(&json_escape(&upsert.id));
            out.push_str("\",\"type\":\"");
            out.push_str(&json_escape(&upsert.node_type));
            out.push('"');
            push_opt_str(&mut out, "title", upsert.title.as_deref());
            push_opt_str(&mut out, "text", upsert.text.as_deref());
            push_opt_str(&mut out, "status", upsert.status.as_deref());
            push_tags(&mut out, &upsert.tags);
            push_opt_meta(&mut out, upsert.meta_json.as_deref());
            out.push('}');
            ("graph_node_upsert", out)
        }
        GraphOp::NodeDelete { id } => (
            "graph_node_delete",
            format!("{{\"op\":\"node_delete\",\"id\":\"{}\"}}", json_escape(id)),
        ),
        GraphOp::EdgeUpsert(upsert) => {
            let mut out = String::new();
            out.push_str("{\"op\":\"edge_upsert\",\"from\":\"");
            out.push_str(&json_escape(&upsert.from));
            out.push_str("\",\"rel\":\"");
            out.push_str(&json_escape(&upsert.rel));
            out.push_str("\",\"to\":\"");
            out.push_str(&json_escape(&upsert.to));
            out.push('"');
            push_opt_meta(&mut out, upsert.meta_json.as_deref());
            out.push('}');
            ("graph_edge_upsert", out)
        }
        GraphOp::EdgeDelete { from, rel, to } => (
            "graph_edge_delete",
            format!(
                "{{\"op\":\"edge_delete\",\"from\":\"{}\",\"rel\":\"{}\",\"to\":\"{}\"}}",
                json_escape(from),
                json_escape(rel),
                json_escape(to)
            ),
        ),
    }
}

fn insert_graph_doc_entry_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    ts_ms: i64,
    op: &GraphOp,
    source_event_id: Option<&str>,
) -> Result<(String, Option<i64>), StoreError> {
    let (event_type, payload_json) = build_graph_op_event(op);
    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, source_event_id, event_type, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace,
            branch,
            doc,
            ts_ms,
            DocEntryKind::Event.as_str(),
            source_event_id,
            event_type,
            &payload_json
        ],
    )?;

    if inserted > 0 {
        Ok((payload_json, Some(tx.last_insert_rowid())))
    } else {
        Ok((payload_json, None))
    }
}

fn insert_graph_node_version_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    seq: i64,
    ts_ms: i64,
    node_id: &str,
    node_type: Option<&str>,
    title: Option<&str>,
    text: Option<&str>,
    tags: &[String],
    status: Option<&str>,
    meta_json: Option<&str>,
    deleted: bool,
) -> Result<(), StoreError> {
    let tags = encode_tags(tags);
    tx.execute(
        r#"
        INSERT INTO graph_node_versions(
          workspace, branch, doc, seq, ts_ms, node_id, node_type, title, text, tags, status, meta_json, deleted
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            workspace,
            branch,
            doc,
            seq,
            ts_ms,
            node_id,
            node_type,
            title,
            text,
            tags,
            status,
            meta_json,
            if deleted { 1i64 } else { 0i64 }
        ],
    )?;
    Ok(())
}

fn insert_graph_edge_version_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    seq: i64,
    ts_ms: i64,
    from_id: &str,
    rel: &str,
    to_id: &str,
    meta_json: Option<&str>,
    deleted: bool,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO graph_edge_versions(
          workspace, branch, doc, seq, ts_ms, from_id, rel, to_id, meta_json, deleted
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            workspace,
            branch,
            doc,
            seq,
            ts_ms,
            from_id,
            rel,
            to_id,
            meta_json,
            if deleted { 1i64 } else { 0i64 }
        ],
    )?;
    Ok(())
}

fn task_graph_node_id(task_id: &str) -> String {
    format!("task:{task_id}")
}

fn step_graph_node_id(step_id: &str) -> String {
    format!("step:{step_id}")
}

fn build_task_graph_meta_json(task_id: &str) -> String {
    format!(
        "{{\"source\":\"tasks\",\"task_id\":\"{}\"}}",
        json_escape(task_id)
    )
}

fn build_step_graph_meta_json(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"source\":\"tasks\",\"task_id\":\"{}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        json_escape(task_id),
        json_escape(&step.step_id),
        json_escape(&step.path)
    )
}

fn task_title_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
) -> Result<String, StoreError> {
    tx.query_row(
        "SELECT title FROM tasks WHERE workspace=?1 AND id=?2",
        params![workspace, task_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or(StoreError::UnknownId)
}

fn step_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
) -> Result<(String, bool), StoreError> {
    let row = tx
        .query_row(
            "SELECT title, completed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace, task_id, step_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((title, completed)) = row else {
        return Err(StoreError::StepNotFound);
    };
    Ok((title, completed != 0))
}

fn graph_upsert_node_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    now_ms: i64,
    node_id: &str,
    node_type: &str,
    title: Option<&str>,
    status: Option<&str>,
    meta_json: Option<&str>,
    source_event_id: &str,
) -> Result<bool, StoreError> {
    validate_graph_node_id(node_id)?;
    validate_graph_type(node_type)?;

    let op = GraphOp::NodeUpsert(GraphNodeUpsert {
        id: node_id.to_string(),
        node_type: node_type.to_string(),
        title: title.map(|v| v.to_string()),
        text: None,
        tags: Vec::new(),
        status: status.map(|v| v.to_string()),
        meta_json: meta_json.map(|v| v.to_string()),
    });
    let (_payload, seq_opt) = insert_graph_doc_entry_tx(
        tx,
        workspace,
        branch,
        doc,
        now_ms,
        &op,
        Some(source_event_id),
    )?;
    let Some(seq) = seq_opt else {
        return Ok(false);
    };

    insert_graph_node_version_tx(
        tx,
        workspace,
        branch,
        doc,
        seq,
        now_ms,
        node_id,
        Some(node_type),
        title,
        None,
        &[],
        status,
        meta_json,
        false,
    )?;
    Ok(true)
}

fn graph_upsert_edge_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    now_ms: i64,
    from: &str,
    rel: &str,
    to: &str,
    meta_json: Option<&str>,
    source_event_id: &str,
) -> Result<bool, StoreError> {
    validate_graph_node_id(from)?;
    validate_graph_node_id(to)?;
    validate_graph_rel(rel)?;

    let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
        from: from.to_string(),
        rel: rel.to_string(),
        to: to.to_string(),
        meta_json: meta_json.map(|v| v.to_string()),
    });
    let (_payload, seq_opt) = insert_graph_doc_entry_tx(
        tx,
        workspace,
        branch,
        doc,
        now_ms,
        &op,
        Some(source_event_id),
    )?;
    let Some(seq) = seq_opt else {
        return Ok(false);
    };

    insert_graph_edge_version_tx(
        tx, workspace, branch, doc, seq, now_ms, from, rel, to, meta_json, false,
    )?;
    Ok(true)
}

fn graph_node_semantic_eq(left: Option<&GraphNodeRow>, right: Option<&GraphNodeRow>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(a), Some(b)) => {
            a.id == b.id
                && a.deleted == b.deleted
                && a.node_type == b.node_type
                && a.title == b.title
                && a.text == b.text
                && a.tags == b.tags
                && a.status == b.status
                && a.meta_json.as_deref().map(str::trim) == b.meta_json.as_deref().map(str::trim)
        }
    }
}

fn graph_edge_semantic_eq(left: Option<&GraphEdgeRow>, right: Option<&GraphEdgeRow>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(a), Some(b)) => {
            a.from == b.from
                && a.rel == b.rel
                && a.to == b.to
                && a.deleted == b.deleted
                && a.meta_json.as_deref().map(str::trim) == b.meta_json.as_deref().map(str::trim)
        }
    }
}

fn branch_base_info_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT base_branch, base_seq FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace, branch],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?)
}

fn base_sources_for_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Vec<BranchSource>, StoreError> {
    let sources = branch_sources_tx(tx, workspace, branch)?;
    Ok(sources.into_iter().skip(1).collect())
}

fn graph_conflict_detail_row_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    conflict_id: &str,
) -> Result<Option<GraphConflictDetailRow>, StoreError> {
    Ok(tx
        .query_row(
            r#"
            SELECT kind, key, from_branch, into_branch, doc, status, created_at_ms, resolved_at_ms,
                   base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
                   base_from_id, base_rel, base_to_id, base_edge_meta_json,
                   theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
                   theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
                   ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
                   ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json
            FROM graph_conflicts
            WHERE workspace=?1 AND conflict_id=?2
            "#,
            params![workspace, conflict_id],
            |row| {
                Ok(GraphConflictDetailRow {
                    kind: row.get(0)?,
                    key: row.get(1)?,
                    from_branch: row.get(2)?,
                    into_branch: row.get(3)?,
                    doc: row.get(4)?,
                    status: row.get(5)?,
                    created_at_ms: row.get(6)?,
                    resolved_at_ms: row.get(7)?,
                    base_seq: row.get(8)?,
                    base_ts_ms: row.get(9)?,
                    base_deleted: row.get(10)?,
                    base_node_type: row.get(11)?,
                    base_title: row.get(12)?,
                    base_text: row.get(13)?,
                    base_tags: row.get(14)?,
                    base_status: row.get(15)?,
                    base_meta_json: row.get(16)?,
                    base_from_id: row.get(17)?,
                    base_rel: row.get(18)?,
                    base_to_id: row.get(19)?,
                    base_edge_meta_json: row.get(20)?,
                    theirs_seq: row.get(21)?,
                    theirs_ts_ms: row.get(22)?,
                    theirs_deleted: row.get(23)?,
                    theirs_node_type: row.get(24)?,
                    theirs_title: row.get(25)?,
                    theirs_text: row.get(26)?,
                    theirs_tags: row.get(27)?,
                    theirs_status: row.get(28)?,
                    theirs_meta_json: row.get(29)?,
                    theirs_from_id: row.get(30)?,
                    theirs_rel: row.get(31)?,
                    theirs_to_id: row.get(32)?,
                    theirs_edge_meta_json: row.get(33)?,
                    ours_seq: row.get(34)?,
                    ours_ts_ms: row.get(35)?,
                    ours_deleted: row.get(36)?,
                    ours_node_type: row.get(37)?,
                    ours_title: row.get(38)?,
                    ours_text: row.get(39)?,
                    ours_tags: row.get(40)?,
                    ours_status: row.get(41)?,
                    ours_meta_json: row.get(42)?,
                    ours_from_id: row.get(43)?,
                    ours_rel: row.get(44)?,
                    ours_to_id: row.get(45)?,
                    ours_edge_meta_json: row.get(46)?,
                })
            },
        )
        .optional()?)
}

fn graph_nodes_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    include_deleted: bool,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let limit = limit.clamp(1, 1000);
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}

fn graph_edges_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    include_deleted: bool,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    let limit = limit.clamp(1, 2000);
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}

fn graph_nodes_all_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    include_deleted: bool,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC");

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}

fn graph_edges_all_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    include_deleted: bool,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC");

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}

fn graph_node_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_id: &str,
) -> Result<Option<GraphNodeRow>, StoreError> {
    let mut sql = String::from(
        "SELECT node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND node_id=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Text(node_id.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" ORDER BY seq DESC LIMIT 1");

    let mut stmt = tx.prepare(&sql)?;
    let row = stmt
        .query_row(params_from_iter(params.iter()), |row| {
            let raw_tags: Option<String> = row.get(3)?;
            let deleted: i64 = row.get(6)?;
            Ok(GraphNodeRow {
                id: node_id.to_string(),
                node_type: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                title: row.get(1)?,
                text: row.get(2)?,
                tags: decode_tags(raw_tags.as_deref()),
                status: row.get(4)?,
                meta_json: row.get(5)?,
                deleted: deleted != 0,
                last_seq: row.get(7)?,
                last_ts_ms: row.get(8)?,
            })
        })
        .optional()?;
    Ok(row)
}

fn graph_edge_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    key: &GraphEdgeKey,
) -> Result<Option<GraphEdgeRow>, StoreError> {
    let mut sql = String::from(
        "SELECT meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND from_id=? AND rel=? AND to_id=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Text(key.from.clone()));
    params.push(SqlValue::Text(key.rel.clone()));
    params.push(SqlValue::Text(key.to.clone()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" ORDER BY seq DESC LIMIT 1");

    let mut stmt = tx.prepare(&sql)?;
    let row = stmt
        .query_row(params_from_iter(params.iter()), |row| {
            let deleted: i64 = row.get(1)?;
            Ok(GraphEdgeRow {
                from: key.from.clone(),
                rel: key.rel.clone(),
                to: key.to.clone(),
                meta_json: row.get(0)?,
                deleted: deleted != 0,
                last_seq: row.get(2)?,
                last_ts_ms: row.get(3)?,
            })
        })
        .optional()?;
    Ok(row)
}

fn graph_nodes_get_map_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_ids: &[String],
    include_deleted: bool,
) -> Result<std::collections::HashMap<String, GraphNodeRow>, StoreError> {
    use std::collections::HashMap;

    if node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND node_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.insert(
            id.clone(),
            GraphNodeRow {
                id,
                node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                title: row.get(2)?,
                text: row.get(3)?,
                tags: decode_tags(raw_tags.as_deref()),
                status: row.get(5)?,
                meta_json: row.get(6)?,
                deleted: deleted != 0,
                last_seq: row.get(8)?,
                last_ts_ms: row.get(9)?,
            },
        );
    }
    Ok(out)
}

fn graph_edges_get_map_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    edge_keys: &[GraphEdgeKey],
    include_deleted: bool,
) -> Result<std::collections::HashMap<GraphEdgeKey, GraphEdgeRow>, StoreError> {
    use std::collections::HashMap;

    if edge_keys.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND (");
    for (i, key) in edge_keys.iter().enumerate() {
        if i != 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(from_id=? AND rel=? AND to_id=?)");
        params.push(SqlValue::Text(key.from.clone()));
        params.push(SqlValue::Text(key.rel.clone()));
        params.push(SqlValue::Text(key.to.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        let from: String = row.get(0)?;
        let rel: String = row.get(1)?;
        let to: String = row.get(2)?;
        let deleted: i64 = row.get(4)?;
        let key = GraphEdgeKey {
            from: from.clone(),
            rel: rel.clone(),
            to: to.clone(),
        };
        out.insert(
            key,
            GraphEdgeRow {
                from,
                rel,
                to,
                meta_json: row.get(3)?,
                deleted: deleted != 0,
                last_seq: row.get(5)?,
                last_ts_ms: row.get(6)?,
            },
        );
    }
    Ok(out)
}

fn graph_edge_keys_for_node_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_id: &str,
) -> Result<Vec<GraphEdgeKey>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, deleted, seq \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND (from_id=? OR to_id=?))");
    params.push(SqlValue::Text(node_id.to_string()));
    params.push(SqlValue::Text(node_id.to_string()));
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.deleted \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq \
         WHERE c.deleted=0",
    );

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(3)?;
        if deleted != 0 {
            continue;
        }
        out.push(GraphEdgeKey {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
        });
    }
    Ok(out)
}

fn graph_nodes_query_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    request: &GraphQueryRequest,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let limit = limit.clamp(1, 200);
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq WHERE 1=1",
    );

    if let Some(ids) = request.ids.as_ref().filter(|v| !v.is_empty()) {
        sql.push_str(" AND c.node_id IN (");
        for (i, id) in ids.iter().enumerate() {
            if i != 0 {
                sql.push(',');
            }
            sql.push('?');
            params.push(SqlValue::Text(id.clone()));
        }
        sql.push(')');
    }

    if let Some(types) = request.types.as_ref().filter(|v| !v.is_empty()) {
        sql.push_str(" AND c.node_type IN (");
        for (i, ty) in types.iter().enumerate() {
            if i != 0 {
                sql.push(',');
            }
            sql.push('?');
            params.push(SqlValue::Text(ty.clone()));
        }
        sql.push(')');
    }

    if let Some(status) = request
        .status
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        sql.push_str(" AND c.status=?");
        params.push(SqlValue::Text(status.to_string()));
    }

    if let Some(tags_any) = request.tags_any.as_ref().filter(|v| !v.is_empty()) {
        let tags_any = normalize_tags(tags_any)?;
        if !tags_any.is_empty() {
            sql.push_str(" AND (");
            for (i, tag) in tags_any.iter().enumerate() {
                if i != 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("COALESCE(c.tags,'') LIKE ?");
                params.push(SqlValue::Text(format!("%\n{}\n%", tag)));
            }
            sql.push(')');
        }
    }

    if let Some(tags_all) = request.tags_all.as_ref().filter(|v| !v.is_empty()) {
        let tags_all = normalize_tags(tags_all)?;
        for tag in tags_all {
            sql.push_str(" AND COALESCE(c.tags,'') LIKE ?");
            params.push(SqlValue::Text(format!("%\n{}\n%", tag)));
        }
    }

    if let Some(text) = request
        .text
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        sql.push_str(
            " AND instr(lower(COALESCE(c.title,'') || '\n' || COALESCE(c.text,'')), lower(?)) > 0",
        );
        params.push(SqlValue::Text(text.to_string()));
    }

    sql.push_str(" ORDER BY c.seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit + 1));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}

fn graph_edges_for_nodes_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_ids: &[String],
    limit: i64,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    if node_ids.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 5000);

    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND from_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str(") AND to_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq \
         ORDER BY c.seq DESC LIMIT ?",
    );
    params.push(SqlValue::Integer(limit));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}

fn graph_diff_candidates_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
) -> Result<Vec<GraphDiffCandidate>, StoreError> {
    let nodes = graph_nodes_tail_tx(tx, workspace, sources, doc, before_seq, limit, true)?;
    let edges = graph_edges_tail_tx(tx, workspace, sources, doc, before_seq, limit, true)?;

    let mut out = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while out.len() < limit as usize && (i < nodes.len() || j < edges.len()) {
        let take_node = match (nodes.get(i), edges.get(j)) {
            (Some(n), Some(e)) => n.last_seq >= e.last_seq,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => false,
        };

        if take_node {
            let node = nodes[i].clone();
            out.push(GraphDiffCandidate::Node { to: node });
            i += 1;
        } else {
            let edge = edges[j].clone();
            out.push(GraphDiffCandidate::Edge {
                key: GraphEdgeKey {
                    from: edge.from.clone(),
                    rel: edge.rel.clone(),
                    to: edge.to.clone(),
                },
                to: edge,
            });
            j += 1;
        }
    }
    Ok(out)
}

fn graph_merge_candidates_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    doc: &str,
    base_cutoff_seq: i64,
    before_seq: i64,
    limit: i64,
) -> Result<Vec<GraphMergeCandidate>, StoreError> {
    let limit = limit.clamp(1, 1000);

    let mut node_stmt = tx.prepare(
        r#"
        WITH latest AS (
          SELECT node_id, MAX(seq) AS max_seq
          FROM graph_node_versions
          WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq > ?4 AND seq < ?5
          GROUP BY node_id
        )
        SELECT v.node_id, v.node_type, v.title, v.text, v.tags, v.status, v.meta_json, v.deleted, v.seq, v.ts_ms
        FROM graph_node_versions v
        JOIN latest l ON v.node_id=l.node_id AND v.seq=l.max_seq
        ORDER BY v.seq DESC
        LIMIT ?6
        "#,
    )?;
    let mut node_rows = node_stmt.query(params![
        workspace,
        from_branch,
        doc,
        base_cutoff_seq,
        before_seq,
        limit
    ])?;
    let mut nodes = Vec::new();
    while let Some(row) = node_rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        nodes.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }

    let mut edge_stmt = tx.prepare(
        r#"
        WITH latest AS (
          SELECT from_id, rel, to_id, MAX(seq) AS max_seq
          FROM graph_edge_versions
          WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq > ?4 AND seq < ?5
          GROUP BY from_id, rel, to_id
        )
        SELECT v.from_id, v.rel, v.to_id, v.meta_json, v.deleted, v.seq, v.ts_ms
        FROM graph_edge_versions v
        JOIN latest l ON v.from_id=l.from_id AND v.rel=l.rel AND v.to_id=l.to_id AND v.seq=l.max_seq
        ORDER BY v.seq DESC
        LIMIT ?6
        "#,
    )?;
    let mut edge_rows = edge_stmt.query(params![
        workspace,
        from_branch,
        doc,
        base_cutoff_seq,
        before_seq,
        limit
    ])?;
    let mut edges = Vec::new();
    while let Some(row) = edge_rows.next()? {
        let deleted: i64 = row.get(4)?;
        edges.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while out.len() < limit as usize && (i < nodes.len() || j < edges.len()) {
        let take_node = match (nodes.get(i), edges.get(j)) {
            (Some(n), Some(e)) => n.last_seq >= e.last_seq,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => false,
        };

        if take_node {
            out.push(GraphMergeCandidate::Node {
                theirs: nodes[i].clone(),
            });
            i += 1;
        } else {
            out.push(GraphMergeCandidate::Edge {
                theirs: edges[j].clone(),
            });
            j += 1;
        }
    }
    Ok(out)
}

fn graph_conflict_id(
    workspace: &str,
    from_branch: &str,
    into_branch: &str,
    doc: &str,
    kind: &str,
    key: &str,
    base_cutoff_seq: i64,
    theirs_seq: i64,
    ours_seq: i64,
) -> String {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    fn update_str(hash: &mut u64, value: &str) {
        for b in value.as_bytes() {
            *hash ^= *b as u64;
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    fn update_i64(hash: &mut u64, value: i64) {
        for b in value.to_le_bytes() {
            *hash ^= b as u64;
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    let mut h1 = FNV_OFFSET;
    let mut h2 = FNV_OFFSET ^ 0x9e3779b97f4a7c15;

    for (hash, offset) in [(&mut h1, 0u8), (&mut h2, 1u8)] {
        update_str(hash, workspace);
        update_str(hash, from_branch);
        update_str(hash, into_branch);
        update_str(hash, doc);
        update_str(hash, kind);
        update_str(hash, key);
        update_i64(hash, base_cutoff_seq);
        update_i64(hash, theirs_seq);
        update_i64(hash, ours_seq);
        *hash ^= offset as u64;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("CONFLICT-{h1:016x}{h2:016x}")
}

fn graph_conflict_create_node_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    into_branch: &str,
    doc: &str,
    base_cutoff_seq: i64,
    key: &str,
    base: Option<&GraphNodeRow>,
    theirs: Option<&GraphNodeRow>,
    ours: Option<&GraphNodeRow>,
    now_ms: i64,
) -> Result<String, StoreError> {
    let theirs_seq = theirs.map(|n| n.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|n| n.last_seq).unwrap_or(0);
    let conflict_id = graph_conflict_id(
        workspace,
        from_branch,
        into_branch,
        doc,
        "node",
        key,
        base_cutoff_seq,
        theirs_seq,
        ours_seq,
    );

    let base_tags = base.and_then(|n| encode_tags(&n.tags));
    let theirs_tags = theirs.and_then(|n| encode_tags(&n.tags));
    let ours_tags = ours.and_then(|n| encode_tags(&n.tags));

    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO graph_conflicts(
          workspace, conflict_id, kind, key, from_branch, into_branch, doc, base_cutoff_seq,
          base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
          base_from_id, base_rel, base_to_id, base_edge_meta_json,
          theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
          theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
          ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
          ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json,
          status, created_at_ms
        )
        VALUES (
          ?1, ?2, 'node', ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
          NULL, NULL, NULL, NULL,
          ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
          NULL, NULL, NULL, NULL,
          ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34,
          NULL, NULL, NULL, NULL,
          'open', ?35
        )
        "#,
        params![
            workspace,
            &conflict_id,
            key,
            from_branch,
            into_branch,
            doc,
            base_cutoff_seq,
            base.map(|n| n.last_seq),
            base.map(|n| n.last_ts_ms),
            base.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            base.map(|n| n.node_type.as_str()),
            base.and_then(|n| n.title.as_deref()),
            base.and_then(|n| n.text.as_deref()),
            base_tags,
            base.and_then(|n| n.status.as_deref()),
            base.and_then(|n| n.meta_json.as_deref()),
            theirs_seq,
            theirs.map(|n| n.last_ts_ms),
            theirs.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            theirs.map(|n| n.node_type.as_str()),
            theirs.and_then(|n| n.title.as_deref()),
            theirs.and_then(|n| n.text.as_deref()),
            theirs_tags,
            theirs.and_then(|n| n.status.as_deref()),
            theirs.and_then(|n| n.meta_json.as_deref()),
            ours_seq,
            ours.map(|n| n.last_ts_ms),
            ours.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            ours.map(|n| n.node_type.as_str()),
            ours.and_then(|n| n.title.as_deref()),
            ours.and_then(|n| n.text.as_deref()),
            ours_tags,
            ours.and_then(|n| n.status.as_deref()),
            ours.and_then(|n| n.meta_json.as_deref()),
            now_ms
        ],
    )?;
    let _ = inserted;
    Ok(conflict_id)
}

fn graph_conflict_create_edge_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    into_branch: &str,
    doc: &str,
    base_cutoff_seq: i64,
    key: &GraphEdgeKey,
    base: Option<&GraphEdgeRow>,
    theirs: Option<&GraphEdgeRow>,
    ours: Option<&GraphEdgeRow>,
    now_ms: i64,
) -> Result<String, StoreError> {
    let key_str = format!("{}|{}|{}", key.from, key.rel, key.to);
    let theirs_seq = theirs.map(|e| e.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|e| e.last_seq).unwrap_or(0);
    let conflict_id = graph_conflict_id(
        workspace,
        from_branch,
        into_branch,
        doc,
        "edge",
        &key_str,
        base_cutoff_seq,
        theirs_seq,
        ours_seq,
    );

    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO graph_conflicts(
          workspace, conflict_id, kind, key, from_branch, into_branch, doc, base_cutoff_seq,
          base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
          base_from_id, base_rel, base_to_id, base_edge_meta_json,
          theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
          theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
          ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
          ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json,
          status, created_at_ms
        )
        VALUES (
          ?1, ?2, 'edge', ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, NULL, NULL, NULL, NULL, NULL, NULL,
          ?11, ?12, ?13, ?14,
          ?15, ?16, ?17, NULL, NULL, NULL, NULL, NULL, NULL,
          ?18, ?19, ?20, ?21,
          ?22, ?23, ?24, NULL, NULL, NULL, NULL, NULL, NULL,
          ?25, ?26, ?27, ?28,
          'open', ?29
        )
        "#,
        params![
            workspace,
            &conflict_id,
            &key_str,
            from_branch,
            into_branch,
            doc,
            base_cutoff_seq,
            base.map(|e| e.last_seq),
            base.map(|e| e.last_ts_ms),
            base.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            base.map(|e| e.from.as_str()),
            base.map(|e| e.rel.as_str()),
            base.map(|e| e.to.as_str()),
            base.and_then(|e| e.meta_json.as_deref()),
            theirs_seq,
            theirs.map(|e| e.last_ts_ms),
            theirs.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            theirs.map(|e| e.from.as_str()),
            theirs.map(|e| e.rel.as_str()),
            theirs.map(|e| e.to.as_str()),
            theirs.and_then(|e| e.meta_json.as_deref()),
            ours_seq,
            ours.map(|e| e.last_ts_ms),
            ours.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            ours.map(|e| e.from.as_str()),
            ours.map(|e| e.rel.as_str()),
            ours.map(|e| e.to.as_str()),
            ours.and_then(|e| e.meta_json.as_deref()),
            now_ms
        ],
    )?;
    let _ = inserted;
    Ok(conflict_id)
}

fn merge_meta_json(
    existing_meta_json: Option<&str>,
    from_branch: &str,
    from_seq: i64,
    from_ts_ms: i64,
) -> String {
    let payload = format!(
        r#"{{"from":"{}","from_seq":{},"from_ts_ms":{}}}"#,
        json_escape(from_branch),
        from_seq,
        from_ts_ms
    );

    let Some(raw) = existing_meta_json else {
        return format!(r#"{{"_merge":{payload}}}"#);
    };

    let trimmed = raw.trim();
    if looks_like_json_object(trimmed) {
        if trimmed == "{}" {
            return format!(r#"{{"_merge":{payload}}}"#);
        }

        if trimmed.contains("\"_merge\"") {
            return format!(r#"{{"_merge":{payload},"_meta":{trimmed}}}"#);
        }

        let mut out = trimmed.to_string();
        out.pop(); // remove trailing '}'
        if !out.trim_end().ends_with('{') {
            out.push(',');
        }
        out.push_str(&format!(r#""_merge":{payload}}}"#));
        return out;
    }

    format!(
        r#"{{"_merge":{payload},"_meta_raw":"{}"}}"#,
        json_escape(trimmed)
    )
}

fn looks_like_json_object(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}')
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<(), StoreError> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
    match conn.execute(&sql, []) {
        Ok(_) => Ok(()),
        Err(err) if is_duplicate_column(&err) => Ok(()),
        Err(err) => Err(StoreError::Sql(err)),
    }
}

fn is_duplicate_column(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(message)) => {
            message.contains("duplicate column name")
        }
        _ => false,
    }
}

fn ensure_workspace_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT OR IGNORE INTO workspaces(workspace, created_at_ms) VALUES (?1, ?2)",
        params![workspace.as_str(), now_ms],
    )?;
    Ok(())
}

fn branch_checkout_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
) -> Result<Option<String>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT branch FROM branch_checkout WHERE workspace=?1",
            params![workspace],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

fn branch_checkout_set_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO branch_checkout(workspace, branch, updated_at_ms)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace) DO UPDATE SET branch=excluded.branch, updated_at_ms=excluded.updated_at_ms
        "#,
        params![workspace, branch, now_ms],
    )?;
    Ok(())
}

fn bootstrap_default_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    now_ms: i64,
) -> Result<bool, StoreError> {
    if branch_exists_tx(tx, workspace, DEFAULT_BRANCH)? {
        return Ok(false);
    }
    let base_seq = doc_entries_head_seq_tx(tx, workspace)?.unwrap_or(0);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO branches(workspace, name, base_branch, base_seq, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![workspace, DEFAULT_BRANCH, DEFAULT_BRANCH, base_seq, now_ms],
    )?;
    branch_checkout_set_tx(tx, workspace, DEFAULT_BRANCH, now_ms)?;
    Ok(true)
}

fn ensure_checkout_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    now_ms: i64,
) -> Result<bool, StoreError> {
    if branch_checkout_get_tx(tx, workspace)?.is_some() {
        return Ok(false);
    }
    if !branch_exists_tx(tx, workspace, branch)? {
        return Ok(false);
    }
    branch_checkout_set_tx(tx, workspace, branch, now_ms)?;
    Ok(true)
}

fn bump_task_revision_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    expected_revision: Option<i64>,
    now_ms: i64,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM tasks WHERE workspace=?1 AND id=?2",
            params![workspace, task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::UnknownId)?;

    if let Some(expected) = expected_revision {
        if expected != current {
            return Err(StoreError::RevisionMismatch {
                expected,
                actual: current,
            });
        }
    }

    let next = current + 1;
    tx.execute(
        "UPDATE tasks SET revision=?3, updated_at_ms=?4 WHERE workspace=?1 AND id=?2",
        params![workspace, task_id, next, now_ms],
    )?;
    Ok(next)
}

fn bump_plan_revision_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    plan_id: &str,
    expected_revision: Option<i64>,
    now_ms: i64,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM plans WHERE workspace=?1 AND id=?2",
            params![workspace, plan_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::UnknownId)?;

    if let Some(expected) = expected_revision {
        if expected != current {
            return Err(StoreError::RevisionMismatch {
                expected,
                actual: current,
            });
        }
    }

    let next = current + 1;
    tx.execute(
        "UPDATE plans SET revision=?3, updated_at_ms=?4 WHERE workspace=?1 AND id=?2",
        params![workspace, plan_id, next, now_ms],
    )?;
    Ok(next)
}

fn task_items_list_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
) -> Result<Vec<String>, StoreError> {
    let mut stmt = tx.prepare(
        r#"
        SELECT text
        FROM task_items
        WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4
        ORDER BY ordinal ASC
        "#,
    )?;
    let rows = stmt.query_map(params![workspace, entity_kind, entity_id, field], |row| {
        row.get::<_, String>(0)
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn step_items_list_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    step_id: &str,
    table: &str,
) -> Result<Vec<String>, StoreError> {
    let sql = match table {
        "step_criteria" => {
            "SELECT text FROM step_criteria WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        "step_tests" => {
            "SELECT text FROM step_tests WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        "step_blockers" => {
            "SELECT text FROM step_blockers WHERE workspace=?1 AND step_id=?2 ORDER BY ordinal ASC"
        }
        _ => return Err(StoreError::InvalidInput("unknown step items table")),
    };
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map(params![workspace, step_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn task_items_replace_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
    items: &[String],
) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM task_items WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4",
        params![workspace, entity_kind, entity_id, field],
    )?;
    for (idx, text) in items.iter().enumerate() {
        tx.execute(
            "INSERT INTO task_items(workspace, entity_kind, entity_id, field, ordinal, text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![workspace, entity_kind, entity_id, field, idx as i64, text],
        )?;
    }
    Ok(())
}

fn task_items_append_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
    items: &[String],
) -> Result<(), StoreError> {
    let max_ordinal: Option<i64> = tx
        .query_row(
            "SELECT MAX(ordinal) FROM task_items WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4",
            params![workspace, entity_kind, entity_id, field],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let mut next = max_ordinal.unwrap_or(-1) + 1;
    for text in items {
        tx.execute(
            "INSERT INTO task_items(workspace, entity_kind, entity_id, field, ordinal, text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![workspace, entity_kind, entity_id, field, next, text],
        )?;
        next += 1;
    }
    Ok(())
}

fn task_items_remove_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    field: &str,
    items: &[String],
) -> Result<(), StoreError> {
    if items.is_empty() {
        return Ok(());
    }
    for text in items {
        tx.execute(
            "DELETE FROM task_items WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND field=?4 AND text=?5",
            params![workspace, entity_kind, entity_id, field, text],
        )?;
    }
    Ok(())
}

fn checkpoint_required_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    entity_kind: &str,
    entity_id: &str,
    checkpoint: &str,
) -> Result<bool, StoreError> {
    let has_note = tx
        .query_row(
            "SELECT 1 FROM checkpoint_notes WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND checkpoint=?4 LIMIT 1",
            params![workspace, entity_kind, entity_id, checkpoint],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if has_note {
        return Ok(true);
    }
    let has_evidence = tx
        .query_row(
            "SELECT 1 FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind=?2 AND entity_id=?3 AND checkpoint=?4 LIMIT 1",
            params![workspace, entity_kind, entity_id, checkpoint],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(has_evidence)
}

fn resolve_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    path: &StepPath,
) -> Result<String, StoreError> {
    let mut parent_step_id: Option<String> = None;
    for ordinal in path.indices() {
        let step_id: Option<String> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL AND ordinal=?3",
                    params![workspace, task_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3 AND ordinal=?4",
                    params![workspace, task_id, parent_step_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
        };

        let Some(step_id) = step_id else {
            return Err(StoreError::StepNotFound);
        };
        parent_step_id = Some(step_id);
    }
    parent_step_id.ok_or(StoreError::StepNotFound)
}

fn step_path_for_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
) -> Result<String, StoreError> {
    let mut ordinals = Vec::new();
    let mut current = step_id.to_string();

    for _ in 0..128 {
        let row = tx
            .query_row(
                "SELECT parent_step_id, ordinal FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace, task_id, current],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((parent, ordinal)) = row else {
            return Err(StoreError::StepNotFound);
        };
        ordinals.push(ordinal as usize);
        match parent {
            None => break,
            Some(parent_id) => current = parent_id,
        }
    }

    ordinals.reverse();
    Ok(ordinals
        .into_iter()
        .map(|i| format!("s:{i}"))
        .collect::<Vec<_>>()
        .join("."))
}

fn resolve_step_selector_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: Option<&str>,
    path: Option<&StepPath>,
) -> Result<(String, String), StoreError> {
    match (step_id, path) {
        (Some(step_id), _) => {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                    params![workspace, task_id, step_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::StepNotFound);
            }
            Ok((
                step_id.to_string(),
                step_path_for_step_id_tx(tx, workspace, task_id, step_id)?,
            ))
        }
        (None, Some(path)) => {
            let step_id = resolve_step_id_tx(tx, workspace, task_id, path)?;
            Ok((step_id, path.to_string()))
        }
        (None, None) => Err(StoreError::InvalidInput("step selector is required")),
    }
}

fn task_node_path_for_parent_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    parent_step_id: &str,
    ordinal: i64,
) -> Result<String, StoreError> {
    let step_path = step_path_for_step_id_tx(tx, workspace, task_id, parent_step_id)?;
    Ok(format!("{step_path}.t:{ordinal}"))
}

fn resolve_task_node_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    parent_step_id: &str,
    ordinal: i64,
) -> Result<String, StoreError> {
    tx.query_row(
        "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3 AND ordinal=?4",
        params![workspace, task_id, parent_step_id, ordinal],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or(StoreError::UnknownId)
}

fn resolve_task_node_selector_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    node_id: Option<&str>,
    parent_path: Option<&StepPath>,
    ordinal: Option<i64>,
) -> Result<(String, String, String, i64), StoreError> {
    match (node_id, parent_path, ordinal) {
        (Some(node_id), _, _) => {
            let row = tx
                .query_row(
                    "SELECT parent_step_id, ordinal FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND node_id=?3",
                    params![workspace, task_id, node_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((parent_step_id, ordinal)) = row else {
                return Err(StoreError::UnknownId);
            };
            let path =
                task_node_path_for_parent_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            Ok((node_id.to_string(), path, parent_step_id, ordinal))
        }
        (None, Some(parent_path), Some(ordinal)) => {
            let parent_step_id = resolve_step_id_tx(tx, workspace, task_id, parent_path)?;
            let node_id =
                resolve_task_node_id_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            let path =
                task_node_path_for_parent_tx(tx, workspace, task_id, &parent_step_id, ordinal)?;
            Ok((node_id, path, parent_step_id, ordinal))
        }
        _ => Err(StoreError::InvalidInput("task node selector is required")),
    }
}

fn collect_step_subtree_ids_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    root_step_id: &str,
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    let mut stack = vec![root_step_id.to_string()];
    while let Some(current) = stack.pop() {
        out.push(current.clone());
        let mut stmt = tx.prepare(
            "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
        )?;
        let rows = stmt.query_map(params![workspace, task_id, current], |row| {
            row.get::<_, String>(0)
        })?;
        for step_id in rows {
            stack.push(step_id?);
        }
    }
    Ok(out)
}

fn delete_task_rows_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
) -> Result<(), StoreError> {
    let step_ids = {
        let mut stmt = tx.prepare("SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2")?;
        let rows = stmt.query_map(params![workspace, task_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for step_id in step_ids.iter() {
        tx.execute(
            "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM step_notes WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;
        tx.execute(
            "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='step' AND entity_id=?2",
            params![workspace, step_id],
        )?;

        let node_ids = {
            let mut stmt = tx.prepare(
                "SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            )?;
            let rows = stmt.query_map(params![workspace, task_id, step_id], |row| {
                row.get::<_, String>(0)
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for node_id in node_ids {
            tx.execute(
                "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
                params![workspace, node_id],
            )?;
        }
        tx.execute(
            "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            params![workspace, task_id, step_id],
        )?;
    }

    tx.execute(
        "DELETE FROM steps WHERE workspace=?1 AND task_id=?2",
        params![workspace, task_id],
    )?;

    let node_ids = {
        let mut stmt =
            tx.prepare("SELECT node_id FROM task_nodes WHERE workspace=?1 AND task_id=?2")?;
        let rows = stmt.query_map(params![workspace, task_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for node_id in node_ids {
        tx.execute(
            "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task_node' AND entity_id=?2",
            params![workspace, node_id],
        )?;
    }
    tx.execute(
        "DELETE FROM task_nodes WHERE workspace=?1 AND task_id=?2",
        params![workspace, task_id],
    )?;

    tx.execute(
        "DELETE FROM task_items WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_artifacts WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_checks WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM evidence_attachments WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM checkpoint_notes WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;
    tx.execute(
        "DELETE FROM checkpoint_evidence WHERE workspace=?1 AND entity_kind='task' AND entity_id=?2",
        params![workspace, task_id],
    )?;

    tx.execute(
        "DELETE FROM tasks WHERE workspace=?1 AND id=?2",
        params![workspace, task_id],
    )?;

    Ok(())
}

fn next_counter_tx(tx: &Transaction<'_>, workspace: &str, name: &str) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT value FROM counters WHERE workspace=?1 AND name=?2",
            params![workspace, name],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let next = current + 1;
    tx.execute(
        r#"
        INSERT INTO counters(workspace, name, value) VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace, name) DO UPDATE SET value=excluded.value
        "#,
        params![workspace, name, next],
    )?;
    Ok(next)
}

fn build_steps_added_payload(
    task_id: &str,
    parent_path: Option<&str>,
    steps: &[StepRef],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"parent_path\":");
    match parent_path {
        None => out.push_str("null"),
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
    }
    out.push_str(",\"steps\":[");
    for (i, step) in steps.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push_str("{\"step_id\":\"");
        out.push_str(&step.step_id);
        out.push_str("\",\"path\":\"");
        out.push_str(&step.path);
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

fn build_task_node_added_payload(
    task_id: &str,
    node_id: &str,
    path: &str,
    parent_path: &str,
) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"node_id\":\"{node_id}\",\"path\":\"{path}\",\"parent_path\":\"{parent_path}\"}}"
    )
}

fn build_task_node_defined_payload(
    task_id: &str,
    node_id: &str,
    path: &str,
    fields: &[&str],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"node_id\":\"");
    out.push_str(node_id);
    out.push_str("\",\"path\":\"");
    out.push_str(path);
    out.push_str("\",\"fields\":[");
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(field);
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn build_task_node_deleted_payload(task_id: &str, node_id: &str, path: &str) -> String {
    format!("{{\"task\":\"{task_id}\",\"node_id\":\"{node_id}\",\"path\":\"{path}\"}}")
}

fn build_step_deleted_payload(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        step.step_id, step.path
    )
}

fn build_task_deleted_payload(task_id: &str, kind: TaskKind) -> String {
    format!("{{\"task\":\"{task_id}\",\"kind\":\"{}\"}}", kind.as_str())
}

fn build_evidence_captured_payload(
    task_id: &str,
    entity_kind: &str,
    entity_id: &str,
    path: Option<&str>,
    artifacts_count: usize,
    checks_count: usize,
    attachments_count: usize,
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"entity_kind\":\"");
    out.push_str(entity_kind);
    out.push_str("\",\"entity_id\":\"");
    out.push_str(entity_id);
    out.push_str("\",\"path\":");
    match path {
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"artifacts\":");
    out.push_str(&artifacts_count.to_string());
    out.push_str(",\"checks\":");
    out.push_str(&checks_count.to_string());
    out.push_str(",\"attachments\":");
    out.push_str(&attachments_count.to_string());
    out.push_str("}");
    out
}

fn build_step_defined_payload(task_id: &str, step: &StepRef, fields: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push_str("\",\"fields\":[");
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(field);
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn build_step_noted_payload(task_id: &str, step: &StepRef, note_seq: i64) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq}}}",
        step.step_id, step.path
    )
}

fn build_step_noted_mirror_meta_json(
    task_id: &str,
    step: &StepRef,
    note_seq: i64,
    event_id: &str,
) -> String {
    format!(
        "{{\"source\":\"tasks_note\",\"task_id\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq},\"event_id\":\"{event_id}\"}}",
        step.step_id, step.path
    )
}

fn build_step_verified_payload(
    task_id: &str,
    step: &StepRef,
    criteria_confirmed: Option<bool>,
    tests_confirmed: Option<bool>,
    security_confirmed: Option<bool>,
    perf_confirmed: Option<bool>,
    docs_confirmed: Option<bool>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push('"');
    if let Some(v) = criteria_confirmed {
        out.push_str(",\"criteria_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = tests_confirmed {
        out.push_str(",\"tests_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = security_confirmed {
        out.push_str(",\"security_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = perf_confirmed {
        out.push_str(",\"perf_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = docs_confirmed {
        out.push_str(",\"docs_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    out.push('}');
    out
}

fn build_step_done_payload(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        step.step_id, step.path
    )
}

fn build_step_reopened_payload(task_id: &str, step: &StepRef, force: bool) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"force\":{}}}",
        step.step_id,
        step.path,
        if force { "true" } else { "false" }
    )
}

fn build_step_block_payload(
    task_id: &str,
    step: &StepRef,
    blocked: bool,
    reason: Option<&str>,
) -> String {
    let mut out = format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"blocked\":{}}}",
        step.step_id,
        step.path,
        if blocked { "true" } else { "false" }
    );
    if let Some(reason) = reason {
        out.pop();
        out.push_str(",\"reason\":\"");
        out.push_str(reason);
        out.push_str("\"}");
    }
    out
}

fn build_undo_redo_payload(
    op_seq: i64,
    intent: &str,
    task_id: Option<&str>,
    path: Option<&str>,
    undo: bool,
) -> String {
    let mut out = String::new();
    out.push_str("{\"op_seq\":");
    out.push_str(&op_seq.to_string());
    out.push_str(",\"intent\":\"");
    out.push_str(intent);
    out.push_str("\",\"undo\":");
    out.push_str(if undo { "true" } else { "false" });
    if let Some(task_id) = task_id {
        out.push_str(",\"task\":\"");
        out.push_str(task_id);
        out.push('"');
    }
    if let Some(path) = path {
        out.push_str(",\"path\":\"");
        out.push_str(path);
        out.push('"');
    }
    out.push('}');
    out
}

fn insert_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    ts_ms: i64,
    task_id: Option<String>,
    path: Option<String>,
    event_type: &str,
    payload_json: &str,
) -> Result<EventRow, StoreError> {
    let task_id_for_return = task_id.clone();
    let path_for_return = path.clone();
    tx.execute(
        r#"
        INSERT INTO events(workspace, ts_ms, task_id, path, type, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![workspace, ts_ms, task_id, path, event_type, payload_json],
    )?;
    let seq = tx.last_insert_rowid();
    Ok(EventRow {
        seq,
        ts_ms,
        task_id: task_id_for_return,
        path: path_for_return,
        event_type: event_type.to_string(),
        payload_json: payload_json.to_string(),
    })
}

fn ops_history_insert_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: Option<&str>,
    path: Option<String>,
    intent: &str,
    payload_json: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    undoable: bool,
    now_ms: i64,
) -> Result<i64, StoreError> {
    tx.execute(
        r#"
        INSERT INTO ops_history(workspace, task_id, path, intent, payload_json, before_json, after_json, undoable, undone, ts_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
        "#,
        params![
            workspace,
            task_id,
            path,
            intent,
            payload_json,
            before_json,
            after_json,
            if undoable { 1i64 } else { 0i64 },
            now_ms
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

fn parse_event_id(event_id: &str) -> Option<i64> {
    let digits = event_id.strip_prefix("evt_")?;
    digits.parse::<i64>().ok()
}

fn parse_plan_or_task_kind(id: &str) -> Result<TaskKind, StoreError> {
    if id.starts_with("PLAN-") {
        Ok(TaskKind::Plan)
    } else if id.starts_with("TASK-") {
        Ok(TaskKind::Task)
    } else {
        Err(StoreError::InvalidInput("task must start with PLAN- or TASK-"))
    }
}

fn parse_json_or_null(value: Option<String>) -> JsonValue {
    match value {
        None => JsonValue::Null,
        Some(raw) => serde_json::from_str(&raw).unwrap_or(JsonValue::Null),
    }
}
