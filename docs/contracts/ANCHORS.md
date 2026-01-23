# Contracts — Anchors (v0)

This file specifies the **meaning-map anchors** surface: stable architecture areas that bind
tasks/steps/notes/cards/evidence/tests to **concrete parts of a system**.

Anchors are **not** file paths. They are semantic identifiers meant to survive refactors.

Related types:

- `AnchorId` — see `TYPES.md`
- visibility tags: `v:canon` / `v:draft` — see `TYPES.md`
 - legacy lane tag: `lane:agent:*` — treated as draft unless promoted (see `TYPES.md`)

## `anchors_list`

List known anchors (bounded).

Input:

```json
{
  "workspace": "acme/repo",
  "text": "storage",
  "kind": "component",
  "status": "active",
  "limit": 50,
  "max_chars": 4000
}
```

Output:

- `{ workspace, anchors:[...], count, has_more, truncated }`

Semantics:

- Ordering is deterministic: `anchors` are sorted by `id` ascending.
- `text` matches against `id` and `title` (case-insensitive, best-effort).
- `limit` is clamped to a safe maximum (server-defined).

## `anchor_snapshot`

Return a bounded, low-noise context slice for an anchor (meaning-first resumption).

Input:

```json
{
  "workspace": "acme/repo",
  "anchor": "a:storage",
  "include_drafts": false,
  "tasks_limit": 10,
  "limit": 30,
  "max_chars": 6000
}
```

Output:

- `{ workspace, anchor, stats, tasks:[...], cards:[...], count, truncated }`

Semantics:

- Default behavior is **canon-first**:
  - includes pinned cards, cards tagged `v:canon`, and canonical card types (`decision|evidence|test`),
  - excludes explicit drafts (`v:draft`) and legacy lane drafts (`lane:agent:*`) unless promoted.
- If the anchor record defines `aliases[]`, the snapshot includes artifacts tagged with any alias ids as well.
- If `include_drafts=true`, draft filtering is disabled.
- Task lens:
  - `tasks[]` is a bounded, deterministic list of **recent tasks touching this anchor** (derived from `anchor_links`).
  - Ordering is deterministic: `last_ts_ms` desc, then `task` asc.
  - If `tasks_limit=0`, the server returns an empty `tasks[]` (skip the task lens entirely).
  - `tasks[]` is best-effort: if a task id exists in history but the task record is missing, `title`/`status` may be `null`.
- Ordering is deterministic (pinned → type priority → recency → id).
- Selection is cross-graph: the snapshot may include cards written into task graphs and map graphs,
  as long as they are tagged with the anchor id (`a:*`).

## `anchors_export`

Export anchors + relations as deterministic text (no GUI).

Input:

```json
{ "workspace": "acme/repo", "format": "mermaid", "max_chars": 12000 }
```

Where:

- `format`: `"mermaid" | "text"`

Output:

- `{ workspace, format, anchors_count, text, truncated }`

Semantics:

- Export is deterministic: anchors sorted by id; relations derived from anchor records.
- Export must not include timestamps or random ids.

## `anchors_rename`

Rename an anchor id without retagging the full history.

This is a **refactor-grade** operation: it is meant for when you realize the anchor id itself
should change (e.g. `a:core` → `a:domain`).

Input:

```json
{
  "workspace": "acme/repo",
  "from": "a:core",
  "to": "a:domain"
}
```

Output:

- `{ workspace, from, to, anchor }`

Semantics:

- Atomic: either the rename completes or nothing changes.
- After rename:
  - the `from` id is removed as an anchor record,
  - an alias mapping `from → to` is created,
  - opening/snapshotting `to` includes history tagged with `from`,
  - opening `from` resolves to `to` (with a warning).
- Relations are preserved:
  - other anchors that referenced `from` via `parent_id` / `depends_on` are updated to `to`.

## `anchors_bootstrap`

One-call “seed the map”: create or update multiple anchors deterministically.

This is designed for project onboarding: the agent (or human) can propose 10–30 anchors and
commit them in one shot.

Input:

```json
{
  "workspace": "acme/repo",
  "anchors": [
    { "id": "a:core", "title": "Core", "kind": "component" },
    { "id": "a:storage", "title": "Storage adapter", "kind": "boundary", "depends_on": ["a:core"] }
  ],
  "max_chars": 8000
}
```

Output:

- `{ workspace, anchors:[{id, created}], count, created, updated, truncated }`

Semantics:

- Deterministic ordering: anchors are processed in `id` ascending order.
- Atomic by default: if any anchor is invalid, the call fails and no anchors are written.
- Intended to be idempotent: re-running the same bootstrap does not create duplicates.

## `anchors_merge`

Merge one or more anchors into a canonical anchor id (scale hygiene).

This is the “prune the map” operation: when you realize two anchors represent the same architecture
area, you merge the old ids into the winner without retagging the full history.

Input:

```json
{
  "workspace": "acme/repo",
  "into": "a:domain",
  "from": ["a:core", "a:foundation"]
}
```

Output:

- `{ workspace, into, from, merged, skipped, anchor }`

Semantics:

- Atomic: either the merge completes or nothing changes.
- For each `from` id:
  - if it exists as an anchor record, it is removed as an anchor record,
  - an alias mapping `from → into` is created,
  - any aliases owned by that `from` anchor are moved to `into`,
  - opening/snapshotting `into` includes history tagged with `from` automatically.
- Relations are preserved:
  - any other anchors that referenced a merged id via `parent_id` / `depends_on` are updated to `into`.
- Idempotent-by-design:
  - if a `from` id already resolves to `into` via alias mapping, it is counted as `skipped` and does not fail the operation.

## `anchors_lint`

Bounded health check for the meaning map (keep it navigable).

Input:

```json
{
  "workspace": "acme/repo",
  "limit": 50,
  "max_chars": 8000
}
```

Output:

- `{ workspace, issues:[...], count, has_more, truncated }`

Each issue is:

- `{ code, severity, anchor, message, hint }`

Semantics:

- Lint is deterministic and bounded:
  - issues are sorted deterministically (`severity` → `code` → `anchor`),
  - the number of issues returned is bounded by `limit` and `max_chars`.
- Intended to prevent “taxonomy explosion”:
  - common signals include orphan anchors (no linked artifacts), unknown `parent_id`/`depends_on`,
    and alias drift in relations.

## `macro_anchor_note`

One-call “bind knowledge to meaning”.

Creates/updates an anchor (if needed) and writes a bounded reasoning artifact tagged with:

- the anchor id (`a:*`),
- optional step scoping tag when `step="focus"` (or a specific step selector) is provided.

Input:

```json
{
  "workspace": "acme/repo",
  "anchor": "a:storage",
  "title": "Storage adapter",
  "kind": "component",
  "aliases": ["a:store"],
  "content": "Decision: keep store single-writer + atomic tx for task mutation + emitted reasoning event.",
  "card_type": "decision",
  "target": "TASK-123",
  "step": "focus",
  "pin": true,
  "visibility": "canon"
}
```

Output:

- `{ workspace, anchor, scope, note }`

Semantics:

- Must be deterministic and bounded.
- Must not require `agent_id` for durable retrieval (compatibility-only).
- If `target`/`step` is provided (and resolves), the card is committed into that entity's reasoning scope
  (so it remains discoverable from task/step views). Otherwise, it is committed into the workspace-level
  anchor registry scope.
