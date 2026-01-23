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

## Visibility model (v0)

BranchMind keeps **everything** (no silent deletion), but controls default noise via explicit visibility tags:

- `v:canon` — visible in smart/default relevance views (durable anchors + active frontier).
- `v:draft` — hidden by default; available on-demand via `include_drafts=true`, `all_lanes=true`, or `view="audit"` (tool-dependent).

Default visibility when a card has no explicit visibility tag:

- `decision|evidence|test|hypothesis|question|update` → `v:canon`
- everything else (e.g. `note`) → `v:draft`

This is intentionally **meaning-first**: visibility is not a memory key. It’s a presentation lens.

## Workspace scoping (MUST)

All stateful tools in this family operate inside an explicit `workspace` (a stable IDE-provided identifier).

- `workspace` is required in v0 to avoid implicit context and silent cross-project writes.

## Focus-first targeting (DX rule)

To keep daily agent usage **cognitively cheap**, tools that accept a `target` may omit it when the workspace focus is set.

- Explicit `target` always wins.
- If no explicit target/scope is provided, the tool uses the current workspace focus (set via `tasks_focus_set`).
- If focus is not set, tools fall back to checkout-based defaults as described per tool.

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

### `init`

Ensures the workspace storage is initialized and bootstraps a default branch.

Input: `{ workspace }`  
Output: `{ workspace, storage_dir, schema_version, checkout, defaults }`

Bootstrap behavior:

- Creates the default branch `main` when the workspace has no branches.
- Sets the workspace checkout to `main` when it is empty.

Defaults:

- `defaults.branch` = `main`
- `defaults.docs.notes` = `notes`
- `defaults.docs.graph` = `graph`
- `defaults.docs.trace` = `trace`

### `status`

Fast storage and workspace snapshot.

Input: `{ workspace }`  
DX note: `workspace` may be omitted when the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`).
Output:

- `{ workspace, schema_version, workspace_exists, checkout?, defaults, golden_path?, last_task_event?, last_event?, last_doc_entry?, workspace_policy? }`
- `last_task_event` shape: `{ event_id, ts, ts_ms }` (task execution events only; `events` table)
- `last_event` shape: `{ event_id, ts, ts_ms }` (deprecated alias for `last_task_event`, kept for compatibility)
- `last_doc_entry` shape: `{ seq, ts, ts_ms, branch, doc, kind }`
- `checkout` is the current checkout branch (or `null` if unset).
- `defaults` match `init` defaults.
- `golden_path` lists the recommended minimal DX flow (macros + snapshot).
- `workspace_policy` is an optional block exposing anti-drift configuration and guard state (best-effort):
  - `{ default_workspace?, workspace_lock?, project_guard_configured?, project_guard_stored?, default_agent_id? }`

### `help`

Agent-first help: protocol semantics, proof conventions, and the daily portal workflow.

Input: `{ max_chars? }`  
Output: a bounded help text (string).

Semantics:

- Read-only, no workspace required.
- Intended as the single place where the system explains tag semantics and “how to use the portals” (to avoid repeating this in every response).

### `notes_commit`

Appends a single note entry to the **notes** document of a target (plan/task), or to an explicit `(branch, doc)`.

Input (one of):

- `{ workspace, target: "PLAN-###"|"TASK-###", content, title?, format?, meta?, agent_id? }`
- `{ workspace, branch, doc, content, title?, format?, meta?, agent_id? }`

`target` may be a string id or `TargetRef` (see `TYPES.md`), e.g. `{ "id": "TASK-001", "kind": "task" }`.

Defaults:

- If `target` is not provided and `branch`/`doc` are omitted:
  - If workspace focus is set, it is used as the implicit `target`.
  - Otherwise the current checkout branch is used.
- If `doc` is omitted in `(branch, doc)` mode, it defaults to `notes`.

Output:

- `{ entry: { seq, ts, branch, doc, kind:"note", title?, format?, meta?, content } }`

### `show`

Reads a bounded slice (tail/pagination) of a document.

Input (one of):

- `{ workspace, target, doc_kind:"notes"|"trace"?, cursor?, limit?, max_chars? }`
- `{ workspace, branch, doc, cursor?, limit?, max_chars? }`

Defaults:

- If `target` is not provided and `branch` is omitted:
  - If workspace focus is set, it is used as the implicit `target`.
  - Otherwise the current checkout branch is used.
- If `doc` is omitted in that mode, it defaults to:
  - `notes` when `doc_kind="notes"`
  - `trace` when `doc_kind="trace"` (default)

Output:

- `{ branch, doc, entries:[...], sequential?, pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- If `branch` has a recorded base, `show` returns the **effective view** (base snapshot + branch entries).
- If `branch` has no base, it returns entries written to that branch only.
- When reading a trace document (`doc_kind="trace"`), `show` may include a derived `sequential` graph (nodes/edges + missing refs) when sequential trace metadata is present (see “Sequential trace graph (derived)”).

### `open`

Open a single artifact by a stable id/reference (no hunting).

Input: `{ workspace, id, limit?, include_drafts?, max_chars? }`

Supported `id` forms (v1):

- `CARD-...` — a think/graph card id (e.g. `CARD-123`, `CARD-AUTO`, `CARD-PUB-...`).
- `<doc>@<seq>` — a doc-entry reference by global `doc_entries.seq` (e.g. `notes@123`, `TASK-001-trace@456`).
- `a:<slug>` — a meaning-map anchor id (e.g. `a:core`, `a:storage`).
- `runner:<id>` — a runner diagnostic ref (opens the current lease + runner-provided meta; read-only).
- `TASK-...` / `PLAN-...` — a stable task/plan id (opens a minimal “navigation lens” derived from `tasks_resume_super`, read-only).
- `JOB-...` — a delegation job id (opens job status + bounded event tail; prompt is opt-in via `include_drafts=true`).
- `JOB-...@<seq>` — a job event ref (opens the exact event + bounded context).

Output (union):

- Card: `{ kind:"card", id:"CARD-...", head:{ seq, ts, ts_ms, branch, doc }, card:{ id, type, title?, text?, status, tags, meta }, edges:{ supports:[...], blocks:[...] }, truncated }`
- Doc entry: `{ kind:"doc_entry", ref:"notes@123", entry:{ seq, ts, ts_ms, branch, doc, kind, title?, format?, meta?, content }, truncated }`
- Anchor: `{ kind:"anchor", id:"a:...", anchor:{ id, title, kind, status, description?, refs:[...], parent_id?, depends_on:[...], created_at_ms, updated_at_ms, registered }, stats:{ links_count, links_has_more }, cards:[...], count, truncated }`
- Runner: `{ kind:"runner", id:"runner:<id>", status:"offline"|"idle"|"live", lease:{ runner_id, status:"idle"|"live", active_job_id?, lease_expires_at_ms, created_at_ms, updated_at_ms, lease_active, expires_in_ms }, meta?, truncated }`
- Task/plan: `{ kind:"task"|"plan", id:"TASK-..."|"PLAN-...", target:{...}, reasoning_ref:{...}, capsule, step_focus?, degradation, truncated }`
- Job: `{ kind:"job", id:"JOB-...", job:{ id, revision, status, title, kind, priority, task_id?, anchor_id?, runner?, summary?, created_at_ms, updated_at_ms, completed_at_ms? }, prompt?, events:[...], has_more_events, truncated }`
- Job event: `{ kind:"job_event", ref:"JOB-...@<seq>", job:{...}, event:{...}, context:{ events:[...], has_more_events }, truncated }`

Semantics:

- Deterministic and bounded by `max_chars` (full text is returned when budget allows; truncation is explicit).
- No “guessing”: `CARD-...` is resolved by id; `<doc>@<seq>` is resolved by `seq` (the `<doc>` prefix is informational and validated when present).
- `a:<slug>` returns an anchor-scoped snapshot (cards linked by the meaning-map index). When the anchor is not registered in the anchors index, `open` may still return a best-effort snapshot (`anchor.registered=false`) derived from `anchor_links` (no automatic writes).
- `runner:<id>` returns a runner lease snapshot. `status=offline` is derived only from lease expiry (`lease_expires_at_ms <= now_ms`), never from job-event heuristics.
- `TASK-...` / `PLAN-...` is read-only: it never changes workspace focus. The output is intentionally small and navigation-oriented (capsule + reasoning refs + optional step focus).
- `JOB-...` is read-only and bounded: it never changes focus, and it never dumps unbounded logs. Use the dedicated job tools (`tasks_jobs_open`) to tune event/prompt inclusion.
- Anchor snapshots are canonical-first by default (`include_drafts=false`): pinned cards, `v:canon`, and `decision/evidence/test` are preferred. Anchor registry notes should be tagged `v:canon` if they must appear in the default lens.
- When opening a task/plan:
  - `include_drafts=false` uses `view="focus_only"` internally (low-noise default).
  - `include_drafts=true` uses `view="audit"` internally (expanded lens).
- On unknown ids/refs, returns `UNKNOWN_ID` with a recovery hint (e.g. “copy id from snapshot delta”).

### `export`

Builds a bounded snapshot for fast IDE/agent resumption: target metadata + reasoning refs + tail of notes and trace.

Input: `{ workspace, target, notes_limit?, trace_limit?, max_chars? }`

Output:

- `{ target, reasoning_ref, notes:{...}, trace:{ entries, sequential?, ... }, truncated }`

Semantics:

- Resolves the canonical branch/docs via `target` (plan/task).
- `notes` and `trace` are equivalent to calling `show` with `cursor=null` and the provided limits.
  - For `trace`, this includes the derived `sequential` graph when sequential metadata is present.
- `max_chars` applies to the whole snapshot payload; truncation must be explicit.
- `notes_limit=0` disables `notes.entries` (returns an empty array).
- `trace_limit=0` disables `trace.entries` (returns an empty array) and yields `trace.sequential=null`.

### `diff`

Directional diff between two branches for a single document.

Input: `{ workspace, from, to, doc?, cursor?, limit?, max_chars? }`

- `doc` defaults to `"notes"`.
- `cursor`/`limit` follow the same semantics as `show` (tail pagination by `seq`).

Output:

- `{ from, to, doc, entries:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- `entries` are those present in the **effective view** of `(to, doc)` but **not** present in the effective view of `(from, doc)`.
- This is a reasoning-log diff (append-only): “removed” does not exist; reverse arguments to see the opposite direction.

### `merge`

Idempotent merge of note entries from one branch into another (VCS-like for reasoning notes).

Input: `{ workspace, from, into?, doc?, cursor?, limit?, dry_run?, merge_to_base? }`

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

### `graph_apply`

Apply a batch of typed graph operations to a target’s graph document or an explicit `(branch, doc)`.

Input (one of):

- `{ workspace, target: "PLAN-###"|"TASK-###", ops:[...]}`
- `{ workspace, branch, doc, ops:[...] }`

Defaults:

- If `target` is not provided and `branch` is omitted:
  - If workspace focus is set, it is used as the implicit `target`.
  - Otherwise the current checkout branch is used.
- If `doc` is omitted in that mode, it defaults to `graph`.

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

### `graph_query`

Query a bounded slice of the effective graph view.

Input (one of):

- `{ workspace, target, ids?, types?, status?, tags_any?, tags_all?, text?, cursor?, limit?, include_edges?, edges_limit?, max_chars? }`
- `{ workspace, branch, doc, ids?, types?, status?, tags_any?, tags_all?, text?, cursor?, limit?, include_edges?, edges_limit?, max_chars? }`

Defaults:

- `include_edges=true`
- `limit=50` (nodes)
- `edges_limit=200`
- If `target` is not provided and `branch` is omitted:
  - If workspace focus is set, it is used as the implicit `target`.
  - Otherwise the current checkout branch is used.
- If `doc` is omitted in that mode, it defaults to `graph`.

Output:

- `{ branch, doc, nodes:[...], edges:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- Nodes are ordered by `last_seq DESC` (recent-first).
- Pagination uses a `cursor` that behaves like `show`: `last_seq < cursor` (tail pagination by seq).
- `edges` are limited to those connecting returned nodes and bounded by `edges_limit`.
- `max_chars` truncates by dropping older nodes/edges first; truncation must be explicit.

### `graph_validate`

Validate invariants of the effective graph view.

Input (one of):

- `{ workspace, target, max_errors?, max_chars? }`
- `{ workspace, branch, doc, max_errors?, max_chars? }`

Defaults:

- If `target` is not provided and `branch` is omitted:
  - If workspace focus is set, it is used as the implicit `target`.
  - Otherwise the current checkout branch is used.
- If `doc` is omitted in that mode, it defaults to `graph`.

Output:

- `{ branch, doc, ok, stats:{ nodes, edges }, errors:[...], truncated? }`

Semantics (v0):

- Every edge endpoint must exist as a non-deleted node in the same effective view.
- Errors are bounded by `max_errors` (default 50).

### `graph_diff`

Directional diff between two branches for a single graph document (patch-style).

Input: `{ workspace, from, to, doc?, cursor?, limit?, max_chars? }`

- `doc` defaults to `"graph"`.
- `cursor`/`limit` follow the same semantics as `show` (tail pagination by `seq`).

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

### `graph_merge`

Merge graph changes from a derived branch back into its base branch.

Input: `{ workspace, from, into?, doc?, cursor?, limit?, dry_run?, merge_to_base? }`

Defaults:

- `doc` defaults to `"graph"`.

Rules:

- `merge_to_base=true` resolves `into` from `from.base_branch` (explicit merge-back path).
- v0 merge still enforces **merge-back into base**; if `into` is not the base branch, it returns `MERGE_NOT_SUPPORTED`.
- `dry_run=true` discovers outcomes without writing; conflicts are returned as previews (`status="preview"`).

Output:

- `{ from, into, doc, merged, skipped, conflicts_created, conflict_ids, conflicts, diff_summary, pagination:{ cursor, next_cursor?, has_more, limit, count } }`

`diff_summary` includes: `{ nodes_changed, edges_changed, node_fields_changed, edge_fields_changed }`.

Semantics:

- Only keys changed on `from` since branching are considered.
- If `into` also changed the same key differently since the base snapshot → create a conflict entity and skip applying that key.
- Conflict discovery must be deterministic and idempotent.

### `graph_conflicts`

List conflicts for a target merge destination.

Input: `{ workspace, into, doc?, status?, cursor?, limit?, max_chars? }`

Defaults:

- `doc` defaults to `"graph"`.

Output:

- `{ into, doc, conflicts:[{ conflict_id, kind, key, status, created_at_ms }], pagination:{...}, truncated }`

### `graph_conflict_show`

Show a single conflict with base/from/into snapshots.

Input: `{ workspace, conflict_id }`

Output:

- `{ conflict:{ conflict_id, kind, key, from, into, doc, status, base, theirs, ours, created_at_ms, resolved_at_ms? } }`

### `graph_conflict_resolve`

Resolve a conflict explicitly and (optionally) apply the chosen state into the destination branch.

Input: `{ workspace, conflict_id, resolution:"use_from"|"use_into" }`

Output:

- `{ conflict_id, status:"resolved", applied }`

Semantics:

- `use_from` writes the `from` snapshot into `into` as a new version (then marks conflict resolved).
- `use_into` keeps the destination state and just marks conflict resolved.

### `branch_create`

Creates a new branch ref from an existing branch snapshot (no copy).

Input: `{ workspace, name, from? }`

- If `from` is omitted, it defaults to the workspace checkout branch.
- If the workspace has no branches and no checkout, the server bootstraps `main` and uses it as the base.

Output: `{ workspace, branch: { name, base_branch, base_seq } }`

### `diagnostics`

Single-shot health check for reasoning + tasks with recovery suggestions.

Input: `{ workspace, target?, task?, plan?, max_chars? }`

Output: `{ workspace, checkout, focus, target, context_health, warnings?, golden_path? }`

Semantics:

- Aggregates reasoning refs + task lint in one call.
- `golden_path` lists the recommended DX flow (macros + snapshot).

### `macro_branch_note`

Daily portal: append a note, optionally creating/switching a branch.

Input: `{ workspace, name?, from?, doc?, content?, template?, goal?, format?, title?, meta?, agent_id? }`
DX note: `workspace` may be omitted when the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`).

Output: `{ workspace, branch, checkout, note }`

Semantics:

- If `name` is provided: ensure branch `name` exists (create it if missing; base is `from` or current checkout) → checkout to `name` → append the note.
  - If `name` already exists, `from` is ignored and the call proceeds as “checkout + append”.
- If `name` is omitted:
  - if `from` is provided: checkout to existing branch `from` → append the note,
  - otherwise: append the note to the current checkout branch.
- `branch.created` is `true` only when a new branch was created in this call.

## Meaning map anchors (v0.6)

Anchors make agents resume by **meaning** (architecture areas), not by code location and not by session identity.

This surface is specified in `ANCHORS.md` (contracts + semantics):

- `anchors_list` — list known anchors (bounded).
- `anchor_snapshot` — anchor-scoped, meaning-first context slice (canon-first by default).
- `anchors_export` — deterministic text export (`mermaid` / `text`).
- `macro_anchor_note` — one-call “bind knowledge to meaning” (upsert anchor + write anchor-tagged artifact).

Daily DX note:

- In the curated daily toolset, `open` supports `id="a:..."` as the “anchor lens” (bounded, canon-first by default).
- Drafts are hidden by default (`v:draft` and legacy `lane:agent:*`); set `include_drafts=true` (alias: `all_lanes=true`) when you explicitly want the full archive for that scope.

### `branch_list`

Lists known branch refs for a workspace (including canonical task/plan branches).

Input: `{ workspace, limit?, max_chars? }`  
Output: `{ workspace, branches:[...], truncated? }`

### `checkout`

Sets the current workspace branch ref (convenience; does not affect tasks).

Input: `{ workspace, ref }`  
Output: `{ workspace, previous?, current }`

### `branch_rename`

Renames a branch ref and updates dependent artifacts (documents, refs, tags, checkout).

Input: `{ workspace, old, new }`  
Output: `{ workspace, previous, current }`

### `branch_delete`

Deletes a branch ref and its stored artifacts if it is safe to remove.

Input: `{ workspace, name }`  
Output: `{ workspace, name, deleted }`

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

### `think_template`

Return a deterministic card skeleton so agents never guess required fields.

Input: `{ workspace, type, max_chars? }`

Output:

- `{ type, supported_types:[...], template:{...}, truncated }`

Semantics:

- Unknown `type` must be a typed error with a recovery hint listing `supported_types`.
- `template` is a JSON object that includes at least: `id`, `type`, `title?`, `text?`, `status?`, `tags?`, `meta?`.

### `think_card`

Atomically commit a thinking card into `trace_doc` and upsert the corresponding node/edges into `graph_doc`.

Input (one of):

- `{ workspace, target?, card, supports?, blocks?, step?, agent_id? }`
- `{ workspace, branch, trace_doc, graph_doc, card, supports?, blocks?, agent_id? }`

Where:

- `card` is either a JSON object or a string:
  - JSON object string (`"{...}"`) is accepted.
  - DSL string is accepted in v0: `key: value` lines (unknown keys preserved under `card.meta`).
  - Plain text string is accepted and treated as `text` with `type="note"`.
- Normalization:
  - `card.id` (aka `card_id`) is optional; when omitted the server auto-generates `CARD-<seq>`.
  - `card.type` defaults to `"note"` when omitted.
  - At least one of `card.title` or `card.text` must be non-empty.
- Optional:
  - `card.status` (default: `"open"`),
  - `card.tags` (default: `[]`),
  - `step` (TASK targets only): either `"focus"`, a `STEP-...` id, or a StepPath (`s:0`, `s:0.s:1`).
  - `agent_id` (optional): accepted for compatibility and best-effort audit metadata. Durable retrieval must not depend on it.
  - `supports[]` / `blocks[]` — arrays of other card ids (graph edges from `card.id`).

Defaults:

- If `target` is omitted and no explicit scope overrides are provided, the workspace `focus` is used (if set).
- If neither `target` nor `focus` is available, the current checkout branch is used.
- If `trace_doc` is omitted in checkout-branch mode, it defaults to `trace`.
- If `graph_doc` is omitted in checkout-branch mode, it defaults to `graph`.

Step scoping:

- When `step` is provided, the server also:
  - adds a canonical tag `step:<lower(step_id)>` to `card.tags`,
  - writes `card.meta.step={ task_id, step_id, path }`.
- `step="focus"` resolves the first open step of the target task (or focused task). If there is no open step, it is a typed error.
- `step` is not supported with explicit `branch`/`*_doc` overrides.

Output:

- `{ branch, trace_doc, graph_doc, card_id, inserted, trace_seq, trace_ref, graph_applied:{ nodes_upserted, edges_upserted }, last_seq?, graph_ref? }`

Semantics:

- **Atomic**: trace entry + graph updates commit as one transaction.
- **Idempotent** by `card_id`:
  - A repeated call with identical normalized `card` + edges must not create a second trace entry.
  - It must not create new graph versions if the effective node/edges are already semantically equal.
 - `trace_ref` is a copy/paste-friendly stable ref for navigation: `"<trace_doc>@<trace_seq>"` (openable via `open`).
 - `graph_ref` is optional: when the graph upsert emits a doc entry seq, it is returned as `"<graph_doc>@<last_seq>"` (openable via `open`).

### `think_pipeline`

Canonical multi-stage pipeline: `frame → hypothesis → test → evidence → decision`.

Input:

```json
{
  "workspace": "acme/repo",
  "target": "TASK-001",
  "agent_id": "agent-1",
  "step": "focus",
  "frame": "...",
  "hypothesis": "...",
  "test": "...",
  "evidence": "...",
  "decision": "...",
  "status": { "hypothesis": "open", "decision": "accepted" },
  "note_decision": true,
  "note_title": "Decision",
  "note_format": "text"
}
```

Semantics:

- Any stage may be omitted, but at least one must be provided.
- Each stage becomes a `think_card` with `card.type=<stage>`.
- Each stage is auto-linked via `supports` to the previous stage (if present).
- If `note_decision=true`, the decision is summarized into `notes_doc`.
- If `note_decision=true` and `step` is provided, the decision note is stamped with `meta.step` (so step-scoped packs can retrieve it deterministically).
- `status` keys must match provided stages; unknown keys or statuses for missing stages are errors.
- If `step` is provided, all stages share the same step scoping (tag + `meta.step`) and `step` follows the same rules as `think_card`.
- If `agent_id` is provided, it may be recorded as best-effort audit metadata. Durable reasoning retrieval must not depend on it.

### `think_context`

Return a bounded, low-noise “thinking context slice” for fast resumption.

Input (one of):

- `{ workspace, target, branch?, view?, step?, limit_cards?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts? }`
- `{ workspace, branch, graph_doc, view?, limit_cards?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts? }`

Defaults:

- `limit_cards=30`
- If `target` is not provided and `branch` is omitted, the current checkout branch is used.
- If `graph_doc` is omitted in that mode, it defaults to `graph`.

Output:

- `{ branch, graph_doc, step_focus?, stats:{ cards, by_type:{...} }, cards:[...], truncated }`

Semantics:

- `view` (optional): `smart | explore | audit`.
  - `smart` — relevance-first + **cold archive** (prioritize pins + open frontier; recent fill uses `status="open"`).
  - `explore` — relevance-first + **warm archive** (recent fill includes any status).
  - `audit` — like `smart`, but includes drafts (explicit opt-in).
  - Default: if `view` is omitted, it defaults to `smart` when `context_budget` is provided; otherwise defaults to `explore` for compatibility.
- `cards[]` are graph nodes filtered to `supported_types`, selected **relevance-first** (stable ordering):
  1) pinned cards (anchors),
  2) open frontier (hypotheses/questions/subgoals/tests),
  3) recent fill (cold/warm depends on `view`).
- `max_chars` truncates by dropping older cards first; truncation must be explicit.
- If a precondition fails (unknown target/branch), return a typed error and a single best next action suggestion.
- Optional `step` (TASK targets only): `"focus"`, `STEP-...`, or a StepPath (`s:0`, `s:0.s:1`).
  - `step="focus"` resolves the first open step of the target/focused task.
  - When `step` is provided, results are filtered to step-scoped cards (via `tags_all=["step:<...>"]`).
  - `step` is not supported with explicit branch/doc overrides (use `target`/focus scope).
- `agent_id` does not filter results in meaning-mode.
- If `all_lanes=true` (or `include_drafts=true`), draft filtering is disabled for this read view (includes `v:draft` and legacy `lane:agent:*` artifacts). Intended for explicit audit/sync.

## Parity tools (v0.2)

### VCS-style notes helpers

These tools provide a lightweight, notes-focused VCS surface. They are wrappers over
`documents` + `doc_entries` and do not alter task semantics.

### `commit`

Appends a note entry and returns a commit-like record.

Input: `{ workspace, artifact, message, docs? }`

Semantics:

- `artifact` is stored as the note `content`.
- `docs` defaults to `notes` when omitted.

### `log`

Returns recent commit-like entries.

Input: `{ workspace, ref?, limit? }`

Defaults:

- `ref` defaults to the current checkout branch.

### `docs_list`

List known documents for a branch/ref.

Input: `{ workspace, ref? }`

### `tag_create` / `tag_list` / `tag_delete`

Create, list, and delete lightweight tags that point to commit entries.

### `reflog`

Returns ref movements for the VCS-style surface.

### `reset`

Moves a ref pointer to a specified commit entry.

## Think convenience tools (v0.2)

Defaults:

- Think tools accept `target` (plan/task) or explicit branch/ref + doc overrides
  (see each tool schema for exact field names).
- When `target` is provided, branch/ref/doc overrides must be omitted and
  branch/docs are resolved from the target reasoning reference.
- If `target` is absent, explicit `branch`/`ref` wins; otherwise fallback to checkout.
- Doc keys default to `notes` / `graph` / `trace` when supported by the tool.

### `think_add_hypothesis` / `think_add_question` / `think_add_test` / `think_add_note` / `think_add_decision` / `think_add_evidence` / `think_add_frame` / `think_add_update`

Thin wrappers over `think_card` that enforce the corresponding `card.type`
and normalize fields.

### `think_query`

Query thinking cards via graph filters.

Input: `{ workspace, target?, graph_doc?, ref?, step?, ids?, status?, tags_any?, tags_all?, text?, limit?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts? }`

Step scoping:

- Optional `step` (TASK targets only): `"focus"`, `STEP-...`, or a StepPath (`s:0`, `s:0.s:1`).
- `step="focus"` resolves the first open step of the target/focused task.
- When `step` is provided, it is equivalent to requiring `tags_all` to include the canonical step tag `step:<lower(step_id)>`.
- `step` is not supported with explicit `ref`/`graph_doc` overrides (use `target`/focus scope).

Optional:

- `context_budget` — alias for `max_chars` (use the smaller of the two when both are provided).
- `agent_id` — accepted for compatibility and audit metadata; it does not filter results in meaning-mode.
- `all_lanes` — when `true`, includes drafts (`v:draft` + legacy `lane:agent:*`) in this read view. Intended for explicit audit/sync.
- `include_drafts` — alias for `all_lanes` (read UX: “show drafts”).

### `think_pack`

Bounded low-noise pack: a compact `think_context` + stats summary.

Optional:

- `view` — relevance view (`smart | explore | audit`), shaping candidate selection and archive temperature (see `think_context` for semantics and defaulting).
- `context_budget` — alias for `max_chars` (use the smaller of the two when both are provided).
- `agent_id` — accepted for compatibility and audit metadata; it does not filter results in meaning-mode.
- `all_lanes` — when `true`, includes drafts (`v:draft` + legacy `lane:agent:*`) in this read view. Intended for explicit audit/sync.
- `include_drafts` — alias for `all_lanes` (read UX: “show drafts”).
- `step` (TASK targets only): `"focus"`, `STEP-...`, or StepPath. When provided, candidates/frontier are filtered to step-scoped cards (via `tags_all=["step:<...>"]`). Not supported with explicit `ref`/`graph_doc` overrides.

Output (optional):

- When `all_lanes=true` (or `include_drafts=true`), the response may include a small `lane_summary` derived from the returned slice (counts + top pinned/open per lane).
- The response includes a small `capsule` HUD (`type="think_pack_capsule"`) with:
  - **where**: workspace + branch/docs + lane + optional step focus,
  - **why**: top engine signals (bounded),
  - **next**: 1 primary + 1 backup action derived from the engine (bounded).
- The response may include a derived `engine` block (signals + actions) computed over the returned cards slice (read-only, deterministic).
- The response includes `trace_doc` to make suggested write calls unambiguous (even though `think_pack` itself does not return trace entries).

### `context_pack`

Bounded resumption pack that merges **notes**, **trace**, and **graph cards** into one response.

Input (one of):

- `{ workspace, target, view?, step?, notes_limit?, trace_limit?, limit_cards?, decisions_limit?, evidence_limit?, blockers_limit?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts?, read_only? }`
- `{ workspace, ref?, view?, notes_doc?, trace_doc?, graph_doc?, notes_limit?, trace_limit?, limit_cards?, decisions_limit?, evidence_limit?, blockers_limit?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts?, read_only? }`

Defaults:

- If `target` is provided, `ref`/`notes_doc`/`trace_doc`/`graph_doc` must be omitted.
- If `ref` is omitted, the current checkout branch is used.
- `notes_doc=notes`, `trace_doc=trace`, `graph_doc=graph`.
- `notes_limit=20`, `trace_limit=50`, `limit_cards=30`.
- `decisions_limit=5`, `evidence_limit=5`, `blockers_limit=5`.

Optional:

- `view` — relevance view (`smart | explore | audit`) for the graph-derived slice (`cards`). `audit` implies drafts-visible mode.
- `context_budget` — alias for `max_chars` (use the smaller of the two when both are provided).
- `step` (TASK targets only): `"focus"`, `STEP-...`, or a StepPath (`s:0`, `s:0.s:1`).
  - `step="focus"` resolves the first open step of the target/focused task.
  - When `step` is provided, the response is step-scoped (room-first):
    - Graph-derived slices (`cards`, `signals.*`) are filtered to step-scoped cards (via `tags_all=["step:<...>"]`).
    - `trace.entries` keeps:
      - note entries whose `meta.step` matches the focus step,
      - event entries whose `{ task_id, path }` match the focus step path prefix.
    - `notes.entries` keeps note entries whose `meta.step` matches the focus step.
  - `step` is not supported with explicit `ref`/`notes_doc`/`trace_doc`/`graph_doc` overrides (use `target`/focus scope).
- `agent_id` — accepted for compatibility and audit metadata; it does not filter results in meaning-mode.
- `all_lanes` — when `true`, includes drafts (`v:draft` + legacy `lane:agent:*`) for graph-derived slices and note-like doc entries (`notes.entries`, trace note entries). Intended for explicit audit/sync.
- `include_drafts` — alias for `all_lanes` (read UX: “show drafts”).
- `notes_limit=0` disables `notes.entries` (returns an empty array).
- `trace_limit=0` disables `trace.entries` (returns an empty array).

Output:

- `{ requested:{ target, ref }, branch, docs:{ notes, trace, graph }, capsule?, engine?, lane_summary?, notes:{...}, trace:{ entries, sequential?, pagination }, cards:[...], signals:{ blockers, decisions, evidence, stats }, stats:{...}, bridge?, truncated }`

Notes:

- If `target` is provided but its scope is empty and the checkout branch has recent context,
  `bridge` is included with `{ checkout, docs, has }` and a `CONTEXT_EMPTY_FOR_TARGET` warning.
- If `read_only=true`, the tool avoids creating missing reasoning refs and derives default docs from the target id.
- The response includes a small `capsule` HUD (`type="context_pack_capsule"`) with stable `where/why/next` blocks.
- `trace.sequential` may be included as a derived branching graph when sequential trace metadata is present (see “Sequential trace graph (derived)” below).
- When `all_lanes=true` (or `include_drafts=true`), the response may include a small `lane_summary` derived from the returned slice (counts + top pinned/open per lane).

### `context_pack_export`

Write a bounded `context_pack` **result** to a file for external tooling.

This is intended for integrations where another system can only read from the project filesystem
(e.g., a repo-scoped search/indexing tool).

Input:

- Same as `context_pack`, plus `out_file` (required).

Output:

- `{ out_file, bytes, truncated }`

Recommended convention (for Context Finder integration):

- Write to `.agents/mcp/context/.context/branchmind/context_pack.json` under the project root.
  - Legacy convention: `.context-finder/branchmind/context_pack.json` (supported by Context Finder).

## Session transcripts (filesystem; read-only)

BranchMind can act as an **explicit, bounded window** into Codex session transcripts stored on disk
(e.g., `rollout-*.jsonl` files under `$CODEX_HOME/sessions/...`).

This surface is intentionally:

- **read-only** (no ingestion into the BranchMind store; no background indexing),
- **explicit** (the caller may override `root_dir` and safety filters; defaults are conservative),
- **bounded** (hard limits on files/bytes/returned chars),
- **anti-drift by default** (optional `cwd_prefix` filter; defaults to the server project root).

### `transcripts_search`

Search across transcript files under a directory.

Input:

- `{ workspace, root_dir?, query, cwd_prefix?, role?, dedupe?, max_files?, max_bytes_total?, hits_limit?, context_chars?, max_chars? }`

Semantics:

- `root_dir` is a directory containing Codex transcript files (recursively).
  - If omitted, defaults to `CODEX_HOME/sessions` when `CODEX_HOME` is set,
    otherwise `~/.codex/sessions` (best-effort).
- Only `*.jsonl` files are considered in v0 (future: opt-in support for `*.prompt-answer.md`).
- `cwd_prefix` filters candidates by session project hints (must start with the prefix):
  - `session_meta.payload.cwd`, plus best-effort path hints extracted from:
    - `session_meta.payload.instructions`, and
    - early `payload.type="message"` items (typically `role="user"` / `role="developer"`).
    Hints are extracted via deterministic patterns such as embedded `<cwd>...</cwd>` blocks or
    "AGENTS.md instructions for ..." lines.
  - If omitted, the tool uses the **repo root derived from the server storage directory** as a default `cwd_prefix` (returned in `filters`).
- `role` (optional) filters to message items with `payload.role` (e.g., `"user"`, `"assistant"`, `"system"`).
- `dedupe=true` (default) de-duplicates repeated hits across files by `(project_id, role, msg_hash)` to reduce copy/paste noise.
- The tool must enforce:
  - `max_files` (cap on files opened),
  - `max_bytes_total` (cap on total bytes read),
  - `max_chars` (cap on response payload).
- Matching is case-sensitive substring match on extracted message text (v0).

Output:

- `{ root_dir, filters:{ cwd_prefix, role?, dedupe }, scanned:{ files, bytes, truncated }, projects:[...], hits:[...], truncated }`

Hit shape (v0):

- `{ ref:{ path, line }, session:{ id?, ts? }, project:{ id, name, confidence }, message:{ role, ts?, snippet } }`

Notes:

- `ref.path` is relative to `root_dir` when possible.
- `snippet` is bounded by `context_chars` and should be safe to display in tool output.
- `projects` is a small summary derived from the scanned slice: `{ id, name, confidence, files, hits }`.
- `scanned.truncated=true` means the scan hit `max_bytes_total` (search is best-effort under budgets).
- Top-level `truncated=true` means the **response payload** was reduced to fit `max_chars`.

### `transcripts_open`

Open a single transcript record by file reference.

Input:

- `{ workspace, root_dir?, ref:{ path, line?, byte? }, before_lines?, after_lines?, max_chars? }`

Output:

- `{ root_dir, ref, session:{ id?, ts?, cwd? }, project:{ id, name, confidence }, entries:[...], truncated }`

Notes:

- The tool must reject path traversal: `ref.path` must resolve within `root_dir`.
- Exactly one of `ref.line` or `ref.byte` must be provided.
  - `ref.line` is a 1-based JSONL line number.
  - `ref.byte` is the 0-based byte offset of the JSONL line start (preferred for huge files).
- Entries are returned as extracted message blocks (best-effort); non-message lines may be omitted.
- This tool is intended for `audit` workflows; it should not be pulled into default portals.
- The response may include low-priority suggestions to capture the opened window into durable memory (`macro_branch_note`), optionally step-scoped when a focused task+step exists.

### `transcripts_digest`

Return a small, low-noise digest of recent transcript "summary" messages for the **current project**
to support fast archaeology ("what happened before I arrived?").

Input:

- `{ workspace, root_dir?, cwd_prefix?, mode?, max_files?, max_bytes_total?, max_items?, max_chars? }`

Defaults:

- `root_dir` defaults to `CODEX_HOME/sessions` when `CODEX_HOME` is set, otherwise `~/.codex/sessions`.
- `cwd_prefix` defaults to the server project root (derived from the server storage directory).
  - Project matching is best-effort and uses the same hint sources as `transcripts_search`
    (session meta + early message hint extraction).
- `max_files=720`, `max_bytes_total=16MiB`, `max_items=6` (bounded low-noise defaults; callers can raise explicitly).
- `mode="summary"`:
  - selects assistant messages that look like a summary block (heuristic; deterministic).
  - `mode="last"` selects the last assistant message per session file (bounded).

Output:

- `{ root_dir, filters:{ cwd_prefix, mode }, scanned:{ files, bytes, truncated }, digest:[...], truncated }`

Digest item shape (v0):

- `{ ref:{ path, line?, byte }, session:{ id?, ts? }, message:{ role:"assistant", ts?, text } }`

Notes:

- Digest is read-only and does not ingest transcript text into the store.
- Intended as a "cold archive" view; use explicit note/evidence commits to publish durable anchors.
- To maximize ROI under small `max_bytes_total`, the scan is **windowed** per file (bounded head for scope hints + bounded tail for candidate messages).
  - `ref.byte` is always returned (stable for immutable JSONL).
  - `ref.line` is best-effort and only included when the tail scan covers the full file (no full-file scans under the budget).
- `scanned.truncated=true` means the scan hit `max_bytes_total` (digest is best-effort under budgets).
- If `digest` is empty and `scanned.truncated=true`, the tool emits a warning and provides copy/paste-ready retry suggestions (raise scan budgets or switch mode).
- Top-level `truncated=true` means the **response payload** was reduced to fit `max_chars`.

### `think_frontier` / `think_next`

Return prioritized candidates for next actions (by recency + status).

Input: `{ workspace, target?, ref?, graph_doc?, step?, limit_*?, context_budget?, max_chars?, agent_id?, all_lanes?, include_drafts? }`

Step scoping:

- Optional `step` (TASK targets only): `"focus"`, `STEP-...`, or StepPath. When provided, the frontier (and `think_next` candidate selection) is filtered to step-scoped cards (via `tags_all=["step:<...>"]`). Not supported with explicit `ref`/`graph_doc` overrides.

Notes:

- If `max_chars` is set, responses include `budget.used_chars` and may truncate lists with minimal summaries.
- Under very small budgets, tools return a minimal frontier/candidate stub (or a `signal` fallback) instead of an empty payload.
- `view` (optional): `smart | explore | audit` (relevance view).
  - `audit` implies drafts-visible mode.
- `agent_id` is accepted for compatibility and audit metadata; it does not filter results in meaning-mode.
- If `all_lanes=true` (or `include_drafts=true`), draft filtering is disabled for this read view (includes `v:draft` + legacy `lane:agent:*` artifacts).

### `think_link` / `think_set_status`

Graph edge creation and status updates for card nodes.

### `think_pin` / `think_pins`

Pin/unpin cards and list pins.

### `think_publish` (canonization, v0.6)

Promote a card into canonical visibility to become a durable anchor.

Input: `{ workspace, target?, ref?, graph_doc?, trace_doc?, card_id, pin?, agent_id? }`

Semantics:

- Reads the current card by `card_id` and writes/updates a deterministic published copy:
  - `published_id = "CARD-PUB-" + <card_id>`.
- The published copy:
  - is tagged as `lane:shared` (legacy convention),
  - carries `meta.published_from={ card_id, lane?, agent_id? }` (best-effort),
  - keeps step tags and other tags, excluding legacy lane tags (rewritten),
  - may add `v:canon` to make canonical intent explicit.
- Idempotent: repeated publish updates the same `published_id`.
- `pin=true` may add the canonical pin tag to the published copy to make it visible in smart views.

### `think_nominal_merge`

Deduplicate highly similar cards into a canonical one (idempotent by `card_id`).

### `think_playbook`

Return a deterministic playbook skeleton by name.

Input: `{ workspace, name, max_chars? }`

Semantics:

- Playbooks are **read-only** helpers; they do not write cards or mutate tasks.
- Built-in names (v0.5, deterministic):
  - `default` — generic “frame → hypothesis → test → evidence → decision”.
  - `strict` — skepticism-first discipline (skeptic-loop: counter-hypothesis → minimal falsifying test → stop criteria; plus simplest viable solution; plus an optional breakthrough loop via a 10x lever + decisive test).
  - `breakthrough` — breakthrough-mode reset (tension → inversion → assumptions → extremes → analogy → 10x lever → decisive test → stop criteria).
  - `debug` — debugging skeleton (may recommend `bisect`).
  - `bisect` — binary search pattern over an ordered search space (commits/flags/config).
  - `criteria_matrix` — A vs B tradeoff matrix (criteria + weights + sensitivity).
  - `experiment` — design a single decisive experiment to raise confidence.
  - `contradiction` — resolve supports vs blocks by a decisive test.
- Unknown names return a generic skeleton (deterministic fallback).

Notes:

- The Reasoning Signal Engine may suggest `think_playbook` calls as a **backup action**
  when it detects classic situations (tradeoffs, contradictions, low-confidence anchors, stuck loops).

### `think_subgoal_open` / `think_subgoal_close`

Open/close a subgoal card that links a parent question to a child trace.

### `think_watch`

Return a bounded watch view (frontier + recent trace steps).

Defaults:

- When `target` is provided, `ref`/`graph_doc`/`trace_doc` must be omitted and
  branch/docs are resolved from the target reasoning reference.
- Read views may also include an optional `engine` block (signals + actions) derived from the returned slice (e.g. contradictions, weak evidence, low-confidence anchors).
  - In reduced toolsets (`core`/`daily`), engine actions may include a `call_method` → `tools/list` disclosure step
    to reveal the minimal toolset tier required before executing `call_tool` actions. This keeps engine actions
    copy/paste-able for clients that enforce “advertised tools only”.

Optional:

- `view` — relevance view (`smart | explore | audit`) for candidate selection and archive temperature (see `think_context` for semantics and defaulting).
- `engine_signals_limit` / `engine_actions_limit` — bound the engine output per view (0 disables).
- `context_budget` — alias for `max_chars` (use the smaller of the two when both are provided).
- `agent_id` — accepted for compatibility and audit metadata; it does not filter results in meaning-mode.
- `all_lanes` — when `true`, includes drafts (`v:draft` + legacy `lane:agent:*`) for this read view. Intended for explicit audit/sync.
- `step` (TASK targets only): `"focus"`, `STEP-...`, or StepPath.
  - When provided, frontier/candidates are filtered to step-scoped cards (via `tags_all=["step:<...>"]`).
  - `trace.entries` is also filtered to the focus step (note entries via `meta.step`; event entries via `{ task_id, path }`).
  - Not supported with explicit `ref`/`graph_doc`/`trace_doc` overrides.
- `trace_limit_steps=0` disables the trace slice (returns an empty array).

HUD invariant:

- `think_watch` includes a small, versioned `capsule` block (`type="watch_capsule"`) that stays useful even under aggressive `max_chars` trimming:
  - **where**: workspace + branch/docs + lane + optional step focus
  - **why**: top engine signals (bounded)
  - **next**: 1 primary + 1 backup action derived from the engine (bounded)

Output (optional):

- When `all_lanes=true` (or `include_drafts=true`), the response may include a small `lane_summary` derived from the returned slice (counts + top pinned/open per lane).

## Trace tools (v0.2)

### `trace_step`

Append a structured trace step entry.

Notes:

- You may include sequential metadata in `meta` (e.g. `thoughtNumber`, `branchFromThought`, `branchId`).
  When present, read views can derive a `sequential` graph from `trace_step` entries too.
- When sequential metadata is present but incomplete (e.g. `branchFromThought` without `thoughtNumber`),
  the tool may emit a warning (read-only lint; does not block writes).
- This lint is intentionally low-noise; warnings may be capped to a small number.
- For canonical sequential tracing (and validation via `trace_validate`), prefer `trace_sequential_step`.

### `trace_sequential_step`

Append a step in a sequential trace (with ordering metadata).

### `trace_hydrate`

Return a bounded trace slice for fast resumption.

### `trace_validate`

Validate trace invariants (ordering, required fields).

### Sequential trace graph (derived)

When trace entries include sequential metadata (e.g. `thoughtNumber`, `branchFromThought`, `branchId`,
`isRevision`, `revisesThought`), read views may include a derived `sequential` graph to make branching explicit.

Supported entry formats:

- `format="trace_sequential_step"` (canonical; validated by `trace_validate`)
- `format="trace_step"` when `meta` includes the sequential keys (low-ceremony fallback)

Shape (summary):

- `sequential.nodes[]`: one node per `thoughtNumber` (includes `seq` for cross-reference).
- `sequential.edges[]`: derived edges:
  - `rel="branch"` from `branchFromThought` → `thoughtNumber`
  - `rel="revision"` from `revisesThought` → `thoughtNumber` (when `isRevision=true`)
- `sequential.missing`: references that were not present in the returned slice (common under pagination/budgets).

Notes:

- The graph is derived from the **returned entries slice** (so it may be partial when paginating).
- Under aggressive budgets, entries may be reduced to summary stubs; the derived graph is filtered accordingly.

Defaults:

- Trace tools accept `target` (plan/task) or explicit `(ref, doc)` / `(doc)` inputs.
- When `target` is provided, branch/doc are resolved from the target reasoning reference
  and overrides must be omitted.

## Reasoning engine (derived, read-only) (v0.5 experimental)

Some bounded views (e.g. `think_watch`, `tasks_resume_super`, `think_pack`, `context_pack`) may include an `engine` block.

Principles:

- **Read-only**: the engine never mutates store state.
- **Deterministic**: identical inputs must produce identical signals/actions (stable ordering).
- **Slice-based**: signals/actions are derived from the returned cards/edges/trace slice, so they can be partial under pagination/budgets.
- **Low-noise**: it emits ranked next actions rather than verbose analysis.

Shape (summary):

- `engine.version` — schema/version marker (string).
- `engine.mode` — optional engine mode marker (e.g. `"step_aware"`).
- `engine.step_tag` — optional canonical step tag (e.g. `"step:step-123"`) when step-aware.
- `engine.signals[]` — bounded list of signals: `{ code, severity, message, refs? }`.
- `engine.actions[]` — bounded list of next actions: `{ kind, priority, title, why?, calls? }`.
- `engine.signals_total` / `engine.actions_total` — totals before applying per-view limits.
- `engine.truncated` — `true` when totals exceed the returned bounded lists.

Step-aware action scoping (v0.6):

- When `engine.mode="step_aware"` and `engine.step_tag` is present, the engine may enrich recovery `calls` so that
  `think_card` suggestions are step-scoped (by adding `step` and/or ensuring the card includes the step tag).
  This keeps strict workflows “portal-first”: the suggested action is directly usable and lands in the correct slice.

### Stable identifiers (v0.5)

The engine uses stable, machine-readable identifiers so clients can build UI/automation without brittle string matching.
New codes may be added; existing codes should remain stable.

Signal codes (non-exhaustive baseline):

- `BM1_CONTRADICTION_SUPPORTS_BLOCKS`
- `BM2_EVIDENCE_WEAK`
- `BM3_DECISION_LOW_CONFIDENCE`
- `BM3_HYPOTHESIS_LOW_CONFIDENCE`
- `BM4_HYPOTHESIS_NO_TEST`
- `BM4_HYPOTHESIS_NO_EVIDENCE`
- `BM5_RUNNABLE_TESTS_FRESH`
- `BM6_ASSUMPTION_NOT_OPEN_BUT_USED`
- `BM8_EVIDENCE_STALE`
- `BM10_STUCK_NO_EVIDENCE`
- `BM10_NO_COUNTER_EDGES`
- `BM_LANE_DECISION_NOT_PUBLISHED`

Notes:

- `BM10_NO_COUNTER_EDGES` ignores cards tagged `counter` (prevents infinite counter-chains).

Action kinds (non-exhaustive baseline):

- `run_test` — run a concrete test and capture evidence (bm_mcp never executes the command; execution is out-of-process).
- `add_test_stub` — create the smallest runnable test stub for a hypothesis.
- `resolve_contradiction` — disambiguate supports vs blocks (usually via a decisive test).
- `use_playbook` — load a deterministic playbook skeleton (e.g. `experiment`, `criteria_matrix`, `debug`).
- `publish_decision` — promote a draft decision into canon (`think_publish`).
- `recheck_assumption` — cascade re-evaluation when assumptions change.
- `add_counter_hypothesis` — steelman a counter-position to reduce confirmation bias.

### Executable tests (BM5) — recommended card convention

To make `test` cards actionable, agents may attach an explicit runnable command.

Accepted conventions (best-effort):

- `card.meta.run.cmd` (string) — preferred structured form.
- `card.meta.cmd` (string) — shorthand form.
- `card.text` containing a line that starts with `CMD:` — fallback form.

bm_mcp never executes commands; it only uses them to rank and suggest “run → capture evidence” actions.

### Draft hygiene (experimental)

Drafts may be marked explicitly via `tags[]=["v:draft"]`.
Legacy compatibility: some stored artifacts may carry `lane:agent:<id>` tags (from older multi-agent lane workflows).
In meaning-mode, both are treated as **draft markers** and are hidden by default outside explicit audit/draft views.

To prevent “lost decisions” (draft knowledge never reaching the canonical resume surface), the engine may emit:

- `signal.code="BM_LANE_DECISION_NOT_PUBLISHED"` for a draft-scoped `decision` (e.g. `v:draft`) that has no published canonical copy in the returned slice.
- `action.kind="publish_decision"` suggesting a deterministic promotion via `think_publish` (usually pinned by default).

## Tool groups (future)

- Repo/workspace:
  - `status` / `init`
  - `branch_create` / `branch_list` / `checkout`
- Versioned artifacts:
  - `notes_commit` (notes-only fast path)
  - `commit` / `log` / `show` / `diff`
- Merges and conflicts:
  - `merge`
  - `graph_merge` + `graph_conflict_show` + `graph_conflict_resolve`
- Graph:
  - `graph_apply` / `graph_query` / `graph_validate` / `graph_diff`
- Thinking structure (optional but powerful):
  - `think_template` / `think_next` / `think_card` / `think_context`

## Output budgets

All read-ish tools must accept `max_bytes`/`max_chars`/`max_lines` and return `truncated=true` when applicable.

Budget invariants:

- Responses include `budget` with `{ max_chars, used_chars, truncated }` when `max_chars` is supplied.
- `used_chars` counts the serialized payload excluding the `budget` field.
- If the payload is reduced past normal truncation, the server emits `BUDGET_MINIMAL` and may return a minimal signal.
- If `max_chars` is below the minimum safe payload size, the server clamps and emits `BUDGET_MIN_CLAMPED`.
