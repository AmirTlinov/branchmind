# Viewer (Tauri) — Contract (v0, local-only)

This document defines the **local-only, read-only** IPC contract between:

- `apps/viewer-tauri/src-tauri` (Rust backend; Tauri commands)
- `apps/viewer-tauri/src` (Vite/React UI)

It is intentionally **not** part of the MCP surface of `bm_mcp` (no HTTP viewer, no remote I/O).

## Core invariants

- **Read-only**: all commands open stores with `SqliteStore::open_read_only`.
- **No outbound network calls**.
- **Budgeted outputs**: list/query endpoints accept `limit`, `max_depth`, `timeout_ms`, etc.
- **Snake case**: JSON fields use `snake_case` (Rust `serde` defaults).

## Commands (Tauri)

Names are stable (breaking changes should be accompanied by a version bump of this document).

### Discovery

- `projects_scan(roots?, max_depth?, limit?, timeout_ms?) -> ProjectDto[]`

### Workspaces / focus

- `workspaces_list(storage_dir, limit, offset) -> WorkspaceDto[]`
- `focus_get(storage_dir, workspace) -> string | null`

### Tasks / plans / steps

- `tasks_list(storage_dir, workspace, limit, offset) -> TaskSummaryDto[]`
- `tasks_get(storage_dir, workspace, id) -> TaskDto | null`
- `plans_get(storage_dir, workspace, id) -> PlanDto | null`
- `reasoning_ref_get(storage_dir, workspace, id, kind="task"|"plan") -> ReasoningRefDto | null`
- `steps_list(storage_dir, workspace, task_id, limit) -> StepListDto[]`
- `steps_detail(storage_dir, workspace, task_id, selector={step_id?, path?}) -> StepDetailDto`
- `task_steps_summary(storage_dir, workspace, task_id) -> TaskStepsSummaryDto`

### Documents (notes / trace)

- `docs_show_tail(storage_dir, workspace, branch, doc, cursor?, limit) -> DocSliceDto`
- `docs_entries_since(storage_dir, workspace, input={branch, doc, since_seq, limit, kind?}) -> DocEntriesSinceDto`

### Graph

- `graph_query(storage_dir, workspace, branch, doc, input=GraphQueryInput) -> GraphSliceDto`
- `graph_diff(storage_dir, workspace, from_branch, to_branch, doc, cursor?, limit) -> GraphDiffSliceDto`
- `architecture_lens_get(storage_dir, workspace, input={scope?, mode?, include_draft?, time_window?, limit?}) -> ArchitectureLensDto`
- `architecture_provenance_get(storage_dir, workspace, input={scope?, node_id, include_draft?, time_window?, limit?}) -> ArchitectureProvenanceDto`
- `architecture_hotspots_get(storage_dir, workspace, input={scope?, include_draft?, time_window?, limit?}) -> ArchitectureHotspotDto[]`

Where:

- `scope.kind ∈ {"workspace","plan","task","anchor"}`
- `scope.id` is required for `kind != "workspace"`
- `mode ∈ {"combined","system","execution","reasoning","risk"}`
- `time_window ∈ {"all","7d","24h"}`

### Search / knowledge

- `tasks_search(storage_dir, workspace, text, limit) -> TasksSearchDto`
- `knowledge_search(storage_dir, workspace, text, limit) -> KnowledgeSearchDto`
- `knowledge_card_get(storage_dir, workspace, card_id) -> GraphNodeDto | null`
- `anchors_list(storage_dir, workspace, text?, kind?, status?, limit) -> AnchorsListDto`

## Errors

Commands return `Result<..., String>` and are mapped to a **single string** (v0 simplicity).
Common prefixes:

- `INVALID_INPUT: ...`
- `scope.id is required for kind=...`
- `UNKNOWN_ID`
- `UNKNOWN_BRANCH`
