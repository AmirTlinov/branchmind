#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use bm_storage::{ReasoningRefRow, SqliteStore, StoreError};

pub(crate) struct ReasoningScope {
    pub(crate) branch: String,
    pub(crate) notes_doc: String,
    pub(crate) graph_doc: String,
    pub(crate) trace_doc: String,
}

pub(crate) struct ReasoningScopeInput {
    pub(crate) target: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) notes_doc: Option<String>,
    pub(crate) graph_doc: Option<String>,
    pub(crate) trace_doc: Option<String>,
}

pub(crate) fn resolve_reasoning_ref_for_read(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    target_id: &str,
    kind: TaskKind,
    read_only: bool,
) -> Result<(ReasoningRefRow, bool), StoreError> {
    if read_only {
        if let Some(row) = store.reasoning_ref_get(workspace, target_id, kind)? {
            return Ok((row, true));
        }
        let derived = ReasoningRef::for_entity(kind, target_id);
        return Ok((
            ReasoningRefRow {
                branch: derived.branch,
                notes_doc: derived.notes_doc,
                graph_doc: derived.graph_doc,
                trace_doc: derived.trace_doc,
            },
            false,
        ));
    }

    let row = store.ensure_reasoning_ref(workspace, target_id, kind)?;
    Ok((row, true))
}
