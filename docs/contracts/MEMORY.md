# Contracts — Memory & Reasoning (v1)

BranchMind’s “memory” is a deterministic set of **durable artifacts** (cards/docs/graphs) that lets
agents resume complex work with low noise and stable refs.

The v1 surface is **portal-first**:

- `think` — reasoning + knowledge cards + meaning map (anchors)
- `graph` — graph operations (query/apply/merge)
- `docs` — read-only-ish document ops (list/show/diff/merge)
- `vcs` — deterministic VCS helpers (e.g. `vcs.branch.create`)
- `open` — open any artifact by stable id/ref (`CARD-*`, `notes_doc@seq`, `TASK-*`, `JOB-*`, `code:*`, …)

All of these are still accessed through the 10 v1 portals; nothing else is a tool.

---

## Why it exists (agent UX)

The goal is that an agent can:

1) **Recall first** before changing a subsystem (cheap, bounded).
2) Make a decision with evidence.
3) Store that decision as a stable artifact (card/doc entry) linked to meaning (`a:*`) and/or execution (`TASK-*`).
4) Resume later in seconds by opening a ref and following `actions[]`.

---

## Portal envelope (think/graph/docs/vcs)

All ops-style portals share the same input shape (see `TYPES.md`):

```json
{
  "workspace": "string?",
  "op": "call|<golden-op>",
  "cmd": "<domain>.* (required only for op=call)",
  "args": { "...": "..." },
  "budget_profile": "portal|default|audit",
  "view": "compact|smart|audit"
}
```

Budgets are strict and deterministic. If output is truncated, it must be explicit (`BUDGET_TRUNCATED` warning).

---

## Code refs (repo line anchors)

BranchMind supports stable code references so agents can attach **exact code slices** to decisions,
evidence, anchors, and handoffs — and re-open them later with one `open` call.

Canonical format:

```text
code:<repo-relative-path>#L<start>-L<end>@sha256:<64-hex>
```

Notes:

- `path` must be **repo-relative** (no leading `/`, no `..` traversal).
- Symlinks are rejected (prevents escaping the repo root).
- Line numbers are **1-based and inclusive**.
- Input may omit the sha suffix; `open` returns the normalized ref with the current sha256.
- If an input sha is present and mismatches current content, `open` emits `CODE_REF_STALE`.

---

## Knowledge cards (think → knowledge.*)

Cards are the primary cross-session memory unit.

### Recommended card text format (CARD)

Keep it short and reuse-friendly:

```text
Claim: <what is true>
Scope: <where it applies>
Apply: <how to use it>
Proof: CMD: <...> | LINK: <...> | FILE: <...>
Expiry: YYYY-MM-DD
```

### Key commands (semantic index)

Use `system` → `schema.get(cmd)` for exact schemas.

- `think.knowledge.recall` (alias: `knowledge.recall`)
  - Input: `{ anchor, limit? }`
  - Output: bounded list of matching cards (canon-first)

- `think.knowledge.upsert` (alias: `knowledge.upsert`)
  - Input: `{ anchor, key, card, ... }`
  - Behavior: creates a new version; `(anchor,key)` always resolves to the latest

- `think.knowledge.lint` (alias: `knowledge.lint`)
  - Purpose: prevent “knowledge junk drawer” drift (duplicate keys, missing expiry, etc.)

---

## Reasoning pipelines (think → reasoning.*)

Reasoning is stored as durable cards/docs, not as ephemeral chat.

Core helpers:

- `think.reasoning.seed` (alias: `reasoning.seed`) — returns a deterministic template frame
- `think.reasoning.pipeline` (alias: `reasoning.pipeline`) — hypothesis → evidence → decision (bounded)

Branching:

- `think.idea.branch.create` (alias: `idea.branch.create`) — explore an alternative track
- `think.idea.branch.merge` (alias: `idea.branch.merge`) — merge findings back into the base narrative

---

## Meaning map (anchors)

Anchors are meaning coordinates (`a:<slug>`), not file paths.

Anchor UX is exposed through `think` commands:

- list/snapshot/export/lint/rename/merge
- recommended write primitive: `think.macro.anchor.note`

Normative details live in `ANCHORS.md`.

---

## Graph / Docs / VCS portals

These portals provide additional durable context layers:

- `graph.query|apply|merge` (long-tail via `op=call cmd=graph.*`)
- `docs.list|show|diff|merge` (golden ops for doc navigation; plus call-only long-tail)
- `vcs.branch.create` (and other call-only helpers)

They are deliberately budgeted and deterministic; they must never fetch the network.

---

## Integration with tasks (single organism)

Tasks and memory are not separate products:

- Every mutating `tasks.*` operation emits a durable reasoning event.
- Tasks have stable reasoning refs (`notes_doc`, `graph_doc`, `trace_doc`) created lazily.
- Conflicts must be discoverable and resolvable via explicit tools/commands.

See `INTEGRATION.md` for the normative contract.
