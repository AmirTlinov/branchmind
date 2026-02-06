# Contracts — Anchors (v1)

Anchors are **meaning coordinates** that bind tasks, steps, and reasoning artifacts to stable
areas of a system.

**Anchors are not file paths.** They are semantic identifiers designed to survive refactors.

## AnchorId

- Format: `a:<slug>`
- Example: `a:storage`, `a:mcp`, `a:core`

The exact validation rules are defined in `TYPES.md`.

## Portal + commands

All anchor operations are exposed via the `think` portal:

```json
{
  "op": "call",
  "cmd": "think.anchor.*",
  "args": { "...": "..." }
}
```

Schemas are discoverable at runtime:

```text
system op=schema.get args={cmd:"think.anchor.snapshot"}
```

## `think.anchor.list`

List known anchors (budgeted).

Determinism:

- Ordering is stable (sorted by `id`).
- Filtering is best-effort, case-insensitive.

## `think.anchor.snapshot`

Return a bounded, low-noise context slice for an anchor (meaning-first resumption).

Selected inputs (see schema for full list):

- `anchor` — `a:<slug>`
- `include_drafts` — opt-in expansion for draft lanes/cards
- `tasks_limit` / `limit` / `max_chars` — budget knobs

Semantics:

- Default behavior is **canon-first**:
  - includes pinned cards and `v:canon` artifacts,
  - excludes explicit drafts unless `include_drafts=true`.
- If the anchor record defines `aliases[]`, the snapshot includes artifacts tagged with any alias id as well.

## `think.macro.anchor.note` (recommended)

Bind meaning with one command: attach a minimal canon note/card to an anchor (and optionally to a task/step).

This is the preferred “write-to-meaning” entrypoint: it keeps the meaning map navigable and makes resume cheap.

## Hygiene commands

Anchors are a shared map; keep it clean:

- `think.anchor.lint` — report hygiene issues (unknown deps, drift)
- `think.anchor.merge` — merge two anchors (preserve history via alias)
- `think.anchor.rename` — rename an anchor (preserve history via alias)
- `think.anchor.export` — export the map (budgeted)
- `think.anchor.bootstrap` — idempotent bootstrap (creates required baseline anchors if missing)

