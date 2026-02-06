# Architecture mental map (anchors + graph)

The goal is to make “where am I / what matters” cheap across sessions.

## Contents

- Anchor-first principle
- Bootstrap anchor set (once per repo)
- Anchor snapshots (meaning-first resume)
- Canon vs draft (noise control)
- Graph usage (bounded, ref-first)
- Code refs (stable line anchors)

## Principle: anchor-first

Use anchors (`a:<slug>`) as meaning coordinates:
- stable across refactors,
- reusable for recall,
- attach knowledge + decisions to anchors.

## Bootstrap anchors (once per repo)

Tool: `mcp__branchmind__think`

```json
{
  "workspace": "my-workspace",
  "op": "call",
  "cmd": "think.anchor.bootstrap",
  "args": {
    "anchors": [
      { "id": "a:core", "title": "Core domain", "kind": "component" },
      { "id": "a:storage", "title": "Storage adapter", "kind": "component" },
      { "id": "a:mcp", "title": "MCP adapter", "kind": "component" },
      { "id": "a:runner", "title": "Delegation runner", "kind": "component" },
      { "id": "a:graph", "title": "Graph layer", "kind": "component" },
      { "id": "a:docs", "title": "Docs/contracts", "kind": "component" }
    ]
  },
  "budget_profile": "portal",
  "view": "compact"
}
```

## Anchor snapshots (meaning-first resume)

Use `think.anchor.snapshot` when you’re about to touch a subsystem:

1) It pulls pinned / canon artifacts first (low noise).
2) It gives you stable refs to open, instead of forcing full scans.

Typical daily flow:

- `think.knowledge.recall(anchor="a:storage", limit=12)`
- `think.anchor.snapshot(anchor="a:storage", max_chars=2000)`

## Attach durable architecture notes to an anchor

Tool: `mcp__branchmind__think`

```json
{
  "workspace": "my-workspace",
  "op": "call",
  "cmd": "think.macro.anchor.note",
  "args": {
    "anchor": "a:core",
    "content": "Claim: ...\\nApply: ...\\nProof: ...\\nExpiry: 2028-01-01",
    "card_type": "knowledge",
    "visibility": "canon",
    "pin": true
  },
  "budget_profile": "portal",
  "view": "compact"
}
```

## Canon vs draft (noise control)

- Store hypotheses and in-progress findings as `v:draft`.
- Promote to `v:canon` only when reused (≥2) or expensive-to-rediscover.
- Prefer pins for “must-see on resume” artifacts.

## Keep the graph useful (not a diagram dump)

Graph is for:
- ownership and dependency direction,
- critical invariants and boundaries,
- merge conflict discoverability.

Use `graph.query` for bounded scans; use `open` on returned refs for details.

## Code refs (stable repo line anchors)

When you reference code in a decision/evidence, prefer stable `code:` refs:

```text
code:<repo-relative-path>#L<start>-L<end>@sha256:<64-hex>
```

If you truly can’t produce a `code:` ref, store at least:

```text
FILE: crates/mcp/src/main.rs:220-420
COMMIT: <sha>
```

Later, these receipts can be migrated into first-class `code:` refs.
