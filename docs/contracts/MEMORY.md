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
- `graph` — typed knowledge graph state (Milestone 4).

Documents are addressed by `(branch, doc)`:

- `branch` is the stable reasoning namespace returned by `tasks_radar.reasoning_ref.branch` (e.g. `task/TASK-001`).
- `doc` is one of the reasoning docs returned by `tasks_radar.reasoning_ref.*_doc` (e.g. `notes`, `TASK-001-trace`).

For graph tools, `doc` defaults to `tasks_radar.reasoning_ref.graph_doc` when `target` is provided.

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

### `branchmind_diff`

Directional diff between two branches for a single document.

Input: `{ workspace, from, to, doc?, cursor?, limit?, max_chars? }`

- `doc` defaults to `"notes"`.
- `cursor`/`limit` follow the same semantics as `branchmind_show` (tail pagination by `seq`).

Output:

- `{ from, to, doc, entries:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- `entries` are those present in the **effective view** of `(to, doc)` but **not** present in the effective view of `(from, doc)`.
- This is a reasoning-log diff (append-only): “removed” does not exist; reverse arguments to see the opposite direction.

### `branchmind_merge`

Idempotent merge of note entries from one branch into another (VCS-like for reasoning notes).

Input: `{ workspace, from, into, doc?, cursor?, limit?, dry_run? }`

- `doc` defaults to `"notes"`.
- `dry_run=true` performs discovery only (no writes).

Output:

- `{ from, into, doc, merged, skipped, pagination:{ cursor, next_cursor?, has_more, limit, count } }`

Semantics:

- Only `kind="note"` entries are merged; other kinds are ignored.
- Merge is idempotent via `source_event_id` on inserted notes: `merge:<from_branch>:<from_seq>`.
- A merge copies content into `into` (it does not rewrite history of `from`).
- Pagination supports large merges: if `has_more=true`, retry with `cursor=next_cursor`.

## Graph model (Milestone 4)

Graph is a typed, queryable layer for linking hypotheses/questions/tests/evidence/decisions.

### State model: versioned, snapshot-safe

Graph state is stored as **versioned entities** (nodes and edges):

- every mutation creates a new version with a monotonic `seq`,
- deletion is a tombstone (`deleted=true`),
- the **effective view** of a graph doc is “latest version per key” inside the branch’s effective sources:
  - derived branches inherit a base snapshot (`base_branch` + `base_seq`) exactly like notes,
  - view selection uses `seq <= cutoff` for inherited sources.

This makes graph reads deterministic and makes branch snapshots consistent without copying.

### Node

Node identity key: `id`.

Minimal fields:

- `id` (string, stable),
- `type` (string),
- `title?` (string),
- `text?` (string),
- `status?` (string),
- `tags?` (string array),
- `meta?` (object),
- `deleted` (boolean),
- `last_seq` / `last_ts_ms` (version metadata).

### Edge

Edge identity key: `(from, rel, to)`.

Minimal fields:

- `from` (node id),
- `rel` (string),
- `to` (node id),
- `meta?` (object),
- `deleted` (boolean),
- `last_seq` / `last_ts_ms` (version metadata).

## Graph tool surface (Milestone 4)

### Type discipline (v0)

Although MCP inputs/outputs are JSON strings, the server treats graph keys as typed domain values:

- Node IDs (`GraphNodeId`) and edge keys (`from`, `rel`, `to`) are strictly validated.
- `type` and `rel` are free-form strings, but must pass validation (no control chars; bounded length; `|` reserved).
- `tags` are normalized (lowercased, deduplicated, stable ordering).
- Reserved namespaces: task projection uses `task:` and `step:` node id prefixes.

### `branchmind_graph_apply`

Apply a batch of typed graph operations to a target’s graph document or an explicit `(branch, doc)`.

Input (one of):

- `{ workspace, target: "PLAN-###"|"TASK-###", ops:[...]}`
- `{ workspace, branch, doc, ops:[...] }`

Where `ops[]` is an array of operations:

- `{"op":"node_upsert","id","type","title"?,"text"?,"status"?,"tags"?,"meta"?}`
- `{"op":"node_delete","id"}`
- `{"op":"edge_upsert","from","rel","to","meta"?}`
- `{"op":"edge_delete","from","rel","to"}`

Output:

- `{ branch, doc, applied:{ nodes_upserted, nodes_deleted, edges_upserted, edges_deleted }, last_seq, last_ts_ms }`

Semantics:

- Atomic: either all ops are applied or none.
- Each op creates a new version (`last_seq` increases monotonically).
- IDs must be non-empty; tools must be deterministic.

### `branchmind_graph_query`

Query a bounded slice of the effective graph view.

Input (one of):

- `{ workspace, target, ids?, types?, status?, tags_any?, tags_all?, text?, cursor?, limit?, include_edges?, edges_limit?, max_chars? }`
- `{ workspace, branch, doc, ids?, types?, status?, tags_any?, tags_all?, text?, cursor?, limit?, include_edges?, edges_limit?, max_chars? }`

Defaults:

- `include_edges=true`
- `limit=50` (nodes)
- `edges_limit=200`

Output:

- `{ branch, doc, nodes:[...], edges:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- Nodes are ordered by `last_seq DESC` (recent-first).
- Pagination uses a `cursor` that behaves like `branchmind_show`: `last_seq < cursor` (tail pagination by seq).
- `edges` are limited to those connecting returned nodes and bounded by `edges_limit`.
- `max_chars` truncates by dropping older nodes/edges first; truncation must be explicit.

### `branchmind_graph_validate`

Validate invariants of the effective graph view.

Input (one of):

- `{ workspace, target, max_errors?, max_chars? }`
- `{ workspace, branch, doc, max_errors?, max_chars? }`

Output:

- `{ branch, doc, ok, stats:{ nodes, edges }, errors:[...], truncated? }`

Semantics (v0):

- Every edge endpoint must exist as a non-deleted node in the same effective view.
- Errors are bounded by `max_errors` (default 50).

### `branchmind_graph_diff`

Directional diff between two branches for a single graph document (patch-style).

Input: `{ workspace, from, to, doc?, cursor?, limit?, max_chars? }`

- `cursor`/`limit` follow the same semantics as `branchmind_show` (tail pagination by `seq`).

Output:

- `{ from, to, doc, changes:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Where `changes[]` contains node/edge states from `to` that differ from `from` (including tombstones):

- `{"kind":"node","id", "to":{...}}`
- `{"kind":"edge","key":{"from","rel","to"}, "to":{...}}`

Semantics:

- Changes are ordered by `to.last_seq DESC` and paginated by `seq`.
- A deletion is represented by `to.deleted=true` (tombstone), not by “absence”.

## Conflicts & merge back (Milestone 4)

Graph merges are 3-way and conflicts are first-class entities.

### `branchmind_graph_merge`

Merge graph changes from a derived branch back into its base branch.

Input: `{ workspace, from, into, doc?, cursor?, limit?, dry_run? }`

Rules:

- v0 merge supports only **merge-back into base**: `from.base_branch == into`.
- `dry_run=true` discovers outcomes without writing.

Output:

- `{ from, into, doc, merged, skipped, conflicts_created, conflict_ids?, pagination:{ cursor, next_cursor?, has_more, limit, count } }`

Semantics:

- Only keys changed on `from` since branching are considered.
- If `into` also changed the same key differently since the base snapshot → create a conflict entity and skip applying that key.
- Conflict discovery must be deterministic and idempotent.

### `branchmind_graph_conflicts`

List conflicts for a target merge destination.

Input: `{ workspace, into, doc?, status?, cursor?, limit?, max_chars? }`

Output:

- `{ into, doc, conflicts:[{ conflict_id, kind, key, status, created_at_ms }], pagination:{...}, truncated }`

### `branchmind_graph_conflict_show`

Show a single conflict with base/from/into snapshots.

Input: `{ workspace, conflict_id }`

Output:

- `{ conflict:{ conflict_id, kind, key, from, into, doc, status, base, theirs, ours, created_at_ms, resolved_at_ms? } }`

### `branchmind_graph_conflict_resolve`

Resolve a conflict explicitly and (optionally) apply the chosen state into the destination branch.

Input: `{ workspace, conflict_id, resolution:"use_from"|"use_into" }`

Output:

- `{ conflict_id, status:"resolved", applied }`

Semantics:

- `use_from` writes the `from` snapshot into `into` as a new version (then marks conflict resolved).
- `use_into` keeps the destination state and just marks conflict resolved.

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

## Thinking tools (Milestone 4.1)

These tools provide a **structured external working memory** without storing hidden chain-of-thought.

Design:

- A thinking “card” is an explicit artifact written by the agent.
- `think_card` writes into:
  - the **trace** document as a `note` entry (chronology),
  - the **graph** document as a typed node + optional edges (structure).
- Idempotency is mandatory: repeating the same `card_id` must be a no-op.

Supported card types (v0):

- `frame`, `hypothesis`, `question`, `test`, `evidence`, `decision`, `note`, `update`

### `branchmind_think_template`

Return a deterministic card skeleton so agents never guess required fields.

Input: `{ workspace, type, max_chars? }`

Output:

- `{ type, supported_types:[...], template:{...}, truncated }`

Semantics:

- Unknown `type` must be a typed error with a recovery hint listing `supported_types`.
- `template` is a JSON object that includes at least: `id`, `type`, `title?`, `text?`, `status?`, `tags?`, `meta?`.

### `branchmind_think_card`

Atomically commit a thinking card into `trace_doc` and upsert the corresponding node/edges into `graph_doc`.

Input (one of):

- `{ workspace, target, branch?, card, supports?, blocks? }`
- `{ workspace, branch, trace_doc, graph_doc, card, supports?, blocks? }`

Where:

- `card` is either a JSON object or a string:
  - JSON object string (`"{...}"`) is accepted.
  - DSL string is accepted in v0: `key: value` lines (unknown keys preserved under `card.meta`).
- Required normalized fields:
  - `card.id` (aka `card_id`) — stable identifier (non-empty),
  - `card.type` — one of `supported_types` (non-empty),
  - at least one of `card.title` or `card.text` must be non-empty.
- Optional:
  - `card.status` (default: `"open"`),
  - `card.tags` (default: `[]`),
  - `supports[]` / `blocks[]` — arrays of other card ids (graph edges from `card.id`).

Output:

- `{ branch, trace_doc, graph_doc, card_id, inserted, graph_applied:{ nodes_upserted, edges_upserted }, last_seq? }`

Semantics:

- **Atomic**: trace entry + graph updates commit as one transaction.
- **Idempotent** by `card_id`:
  - A repeated call with identical normalized `card` + edges must not create a second trace entry.
  - It must not create new graph versions if the effective node/edges are already semantically equal.

### `branchmind_think_context`

Return a bounded, low-noise “thinking context slice” for fast resumption.

Input (one of):

- `{ workspace, target, branch?, limit_cards?, max_chars? }`
- `{ workspace, branch, graph_doc, limit_cards?, max_chars? }`

Defaults:

- `limit_cards=30`

Output:

- `{ branch, graph_doc, stats:{ cards, by_type:{...} }, cards:[...], truncated }`

Semantics:

- `cards[]` are graph nodes filtered to `supported_types`, ordered by `last_seq DESC`.
- `max_chars` truncates by dropping older cards first; truncation must be explicit.
- If a precondition fails (unknown target/branch), return a typed error and a single best next action suggestion.

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
