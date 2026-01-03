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

- `{ workspace, schema_version, workspace_exists, checkout?, defaults, golden_path?, last_event?, last_doc_entry? }`
- `last_event` shape: `{ event_id, ts, ts_ms }`
- `last_doc_entry` shape: `{ seq, ts, ts_ms, branch, doc, kind }`
- `checkout` is the current checkout branch (or `null` if unset).
- `defaults` match `init` defaults.
- `golden_path` lists the recommended minimal DX flow (macros + snapshot).

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

- `{ workspace, target: "PLAN-###"|"TASK-###", content, title?, format?, meta? }`
- `{ workspace, branch, doc, content, title?, format?, meta? }`

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

- `{ branch, doc, entries:[...], pagination:{ cursor, next_cursor?, has_more, limit, count }, truncated }`

Semantics:

- If `branch` has a recorded base, `show` returns the **effective view** (base snapshot + branch entries).
- If `branch` has no base, it returns entries written to that branch only.

### `export`

Builds a bounded snapshot for fast IDE/agent resumption: target metadata + reasoning refs + tail of notes and trace.

Input: `{ workspace, target, notes_limit?, trace_limit?, max_chars? }`

Output:

- `{ target, reasoning_ref, notes:{...}, trace:{...}, truncated }`

Semantics:

- Resolves the canonical branch/docs via `target` (plan/task).
- `notes` and `trace` are equivalent to calling `show` with `cursor=null` and the provided limits.
- `max_chars` applies to the whole snapshot payload; truncation must be explicit.

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

Input: `{ workspace, name?, from?, doc?, content?, template?, goal?, format?, title?, meta? }`
DX note: `workspace` may be omitted when the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`).

Output: `{ workspace, branch, checkout, note }`

Semantics:

- If `name` is provided: create branch `name` (base `from` or current checkout) → checkout to `name` → append the note.
- If `name` is omitted:
  - if `from` is provided: checkout to existing branch `from` → append the note,
  - otherwise: append the note to the current checkout branch.
- `branch.created` is `true` only when a new branch was created in this call.

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

- `{ workspace, target, branch?, card, supports?, blocks? }`
- `{ workspace, branch, trace_doc, graph_doc, card, supports?, blocks? }`

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
  - `supports[]` / `blocks[]` — arrays of other card ids (graph edges from `card.id`).

Defaults:

- If `target` is not provided and `branch` is omitted, the current checkout branch is used.
- If `trace_doc` is omitted in that mode, it defaults to `trace`.
- If `graph_doc` is omitted in that mode, it defaults to `graph`.

Output:

- `{ branch, trace_doc, graph_doc, card_id, inserted, graph_applied:{ nodes_upserted, edges_upserted }, last_seq? }`

Semantics:

- **Atomic**: trace entry + graph updates commit as one transaction.
- **Idempotent** by `card_id`:
  - A repeated call with identical normalized `card` + edges must not create a second trace entry.
  - It must not create new graph versions if the effective node/edges are already semantically equal.

### `think_pipeline`

Canonical multi-stage pipeline: `frame → hypothesis → test → evidence → decision`.

Input:

```json
{
  "workspace": "acme/repo",
  "target": "TASK-001",
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
- `status` keys must match provided stages; unknown keys or statuses for missing stages are errors.

### `think_context`

Return a bounded, low-noise “thinking context slice” for fast resumption.

Input (one of):

- `{ workspace, target, branch?, limit_cards?, max_chars? }`
- `{ workspace, branch, graph_doc, limit_cards?, max_chars? }`

Defaults:

- `limit_cards=30`
- If `target` is not provided and `branch` is omitted, the current checkout branch is used.
- If `graph_doc` is omitted in that mode, it defaults to `graph`.

Output:

- `{ branch, graph_doc, stats:{ cards, by_type:{...} }, cards:[...], truncated }`

Semantics:

- `cards[]` are graph nodes filtered to `supported_types`, ordered by `last_seq DESC`.
- `max_chars` truncates by dropping older cards first; truncation must be explicit.
- If a precondition fails (unknown target/branch), return a typed error and a single best next action suggestion.

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

Input: `{ workspace, target?, graph_doc?, ref?, ids?, status?, tags_any?, tags_all?, text?, limit?, max_chars? }`

### `think_pack`

Bounded low-noise pack: a compact `think_context` + stats summary.

### `context_pack`

Bounded resumption pack that merges **notes**, **trace**, and **graph cards** into one response.

Input (one of):

- `{ workspace, target, notes_limit?, trace_limit?, limit_cards?, decisions_limit?, evidence_limit?, blockers_limit?, max_chars?, read_only? }`
- `{ workspace, ref?, notes_doc?, trace_doc?, graph_doc?, notes_limit?, trace_limit?, limit_cards?, decisions_limit?, evidence_limit?, blockers_limit?, max_chars?, read_only? }`

Defaults:

- If `target` is provided, `ref`/`notes_doc`/`trace_doc`/`graph_doc` must be omitted.
- If `ref` is omitted, the current checkout branch is used.
- `notes_doc=notes`, `trace_doc=trace`, `graph_doc=graph`.
- `notes_limit=20`, `trace_limit=50`, `limit_cards=30`.
- `decisions_limit=5`, `evidence_limit=5`, `blockers_limit=5`.

Output:

- `{ requested:{ target, ref }, branch, docs:{ notes, trace, graph }, notes:{...}, trace:{...}, cards:[...], signals:{ blockers, decisions, evidence, stats }, stats:{...}, bridge?, truncated }`

Notes:

- If `target` is provided but its scope is empty and the checkout branch has recent context,
  `bridge` is included with `{ checkout, docs, has }` and a `CONTEXT_EMPTY_FOR_TARGET` warning.
- If `read_only=true`, the tool avoids creating missing reasoning refs and derives default docs from the target id.

### `think_frontier` / `think_next`

Return prioritized candidates for next actions (by recency + status).

Input: `{ workspace, target?, ref?, graph_doc?, limit_*?, max_chars? }`

Notes:

- If `max_chars` is set, responses include `budget.used_chars` and may truncate lists with minimal summaries.
- Under very small budgets, tools return a minimal frontier/candidate stub (or a `signal` fallback) instead of an empty payload.

### `think_link` / `think_set_status`

Graph edge creation and status updates for card nodes.

### `think_pin` / `think_pins`

Pin/unpin cards and list pins.

### `think_nominal_merge`

Deduplicate highly similar cards into a canonical one (idempotent by `card_id`).

### `think_playbook`

Return a deterministic playbook skeleton by name.

### `think_subgoal_open` / `think_subgoal_close`

Open/close a subgoal card that links a parent question to a child trace.

### `think_watch`

Return a bounded watch view (frontier + recent trace steps).

Defaults:

- When `target` is provided, `ref`/`graph_doc`/`trace_doc` must be omitted and
  branch/docs are resolved from the target reasoning reference.

## Trace tools (v0.2)

### `trace_step`

Append a structured trace step entry.

### `trace_sequential_step`

Append a step in a sequential trace (with ordering metadata).

### `trace_hydrate`

Return a bounded trace slice for fast resumption.

### `trace_validate`

Validate trace invariants (ordering, required fields).

Defaults:

- Trace tools accept `target` (plan/task) or explicit `(ref, doc)` / `(doc)` inputs.
- When `target` is provided, branch/doc are resolved from the target reasoning reference
  and overrides must be omitted.

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
