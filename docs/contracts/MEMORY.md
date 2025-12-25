# Contracts — Reasoning Memory Surface (v0)

The reasoning subsystem is a durable, agent-first working memory:

- append-only logs for “what happened and why”,
- branching and merging for what-if exploration,
- typed graph for linking hypotheses/questions/tests/evidence/decisions.

Milestone 2 (MVP) starts with **durable logs** (notes + trace) and strict output budgets.

## Principles

- Store explicit artifacts only (notes, decisions, evidence, diffs).
- Default retrieval is low-noise (log/diff/summary).
- Full artifacts are opt-in and bounded.

## Workspace scoping (MUST)

All stateful tools in this family operate inside an explicit `workspace` (a stable IDE-provided identifier).

- `workspace` is required in v0 to avoid implicit context and silent cross-project writes.

## Document model (v0 MVP)

In v0, the primary memory primitives are **documents**:

- `notes` — human-authored, append-only entries (decisions, rationale, evidence links).
- `trace` — machine-authored, append-only entries (task mutation events ingested automatically).

Documents are addressed by `(branch, doc)`:

- `branch` is the stable reasoning namespace returned by `tasks_radar.reasoning_ref.branch` (e.g. `task/TASK-001`).
- `doc` is one of the reasoning docs returned by `tasks_radar.reasoning_ref.*_doc` (e.g. `notes`, `TASK-001-trace`).

## Branching model (Milestone 3 core)

Branching exists to support **what-if reasoning** without corrupting the canonical history.

Key rules:

- The canonical branch for plans/tasks is `tasks_radar.reasoning_ref.branch` (e.g. `task/TASK-001`).
- `tasks_*` mutations always ingest into the canonical branch (single organism invariant).
- Branching and checkout affect only **reasoning reads/writes** (notes/graph), not task state.

### Base snapshot (no-copy)

Creating a branch records:

- `base_branch` — the source branch name,
- `base_seq` — a cutoff on the global `doc_entries.seq` at the time of branching.

When reading a document on a derived branch, the effective view is:

- all entries from `base_branch` with `seq <= base_seq`,
- plus all entries written directly to the derived branch.

This produces a snapshot-like experience without copying logs.

## Tool surface (Milestone 2–3 MVP)

### `branchmind_init`

Ensures the workspace storage is initialized.

Input: `{ workspace }`  
Output: `{ workspace, storage_dir, schema_version }`

### `branchmind_status`

Fast storage and workspace snapshot.

Input: `{ workspace }`  
Output: `{ workspace, schema_version, last_event_id?, last_event_ts? }`

### `branchmind_notes_commit`

Appends a single note entry to the **notes** document of a target (plan/task), or to an explicit `(branch, doc)`.

Input (one of):

- `{ workspace, target: "PLAN-###"|"TASK-###", content, title?, format?, meta? }`
- `{ workspace, branch, doc, content, title?, format?, meta? }`

Output:

- `{ entry: { seq, ts, branch, doc, kind:"note", title?, format?, meta?, content } }`

### `branchmind_show`

Reads a bounded slice (tail/pagination) of a document.

Input (one of):

- `{ workspace, target, doc_kind:"notes"|"trace"?, cursor?, limit?, max_chars? }`
- `{ workspace, branch, doc, cursor?, limit?, max_chars? }`

Output:

- `{ branch, doc, entries:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- If `branch` has a recorded base, `branchmind_show` returns the **effective view** (base snapshot + branch entries).
- If `branch` has no base, it returns entries written to that branch only.

### `branchmind_export`

Builds a bounded snapshot for fast IDE/agent resumption: target metadata + reasoning refs + tail of notes and trace.

Input: `{ workspace, target, notes_limit?, trace_limit?, max_chars? }`

Output:

- `{ target, reasoning_ref, notes:{...}, trace:{...}, truncated }`

Semantics:

- Resolves the canonical branch/docs via `target` (plan/task).
- `notes` and `trace` are equivalent to calling `branchmind_show` with `cursor=null` and the provided limits.
- `max_chars` applies to the whole snapshot payload; truncation must be explicit.

### `branchmind_branch_create`

Creates a new branch ref from an existing branch snapshot (no copy).

Input: `{ workspace, name, from? }`

- If `from` is omitted, it defaults to the workspace checkout branch.

Output: `{ workspace, branch: { name, base_branch, base_seq } }`

### `branchmind_branch_list`

Lists known branch refs for a workspace (including canonical task/plan branches).

Input: `{ workspace, limit?, max_chars? }`  
Output: `{ workspace, branches:[...], truncated? }`

### `branchmind_checkout`

Sets the current workspace branch ref (convenience; does not affect tasks).

Input: `{ workspace, ref }`  
Output: `{ workspace, previous?, current }`

## Automatic ingestion (single organism invariant)

Every mutating `tasks_*` operation must be ingested into the **trace** document of the same entity:

- the task mutation event is emitted to `tasks_delta` as usual,
- the same `event_id` is written (idempotently) as a `trace` entry inside the reasoning store.

Idempotency rule:

- ingesting the same `event_id` twice must not create duplicates.

## Tool groups (future)

- Repo/workspace:
  - `branchmind_status` / `branchmind_init`
  - `branchmind_branch_create` / `branchmind_branch_list` / `branchmind_checkout`
- Versioned artifacts:
  - `branchmind_notes_commit` (notes-only fast path)
  - `branchmind_commit` / `branchmind_log` / `branchmind_show` / `branchmind_diff`
- Merges and conflicts:
  - `branchmind_merge`
  - `branchmind_graph_merge` + `branchmind_graph_conflict_show` + `branchmind_graph_conflict_resolve`
- Graph:
  - `branchmind_graph_apply` / `branchmind_graph_query` / `branchmind_graph_validate` / `branchmind_graph_diff`
- Thinking structure (optional but powerful):
  - `branchmind_think_template` / `branchmind_think_next` / `branchmind_think_card` / `branchmind_think_context`

## Output budgets

All read-ish tools must accept `max_bytes`/`max_chars`/`max_lines` and return `truncated=true` when applicable.
