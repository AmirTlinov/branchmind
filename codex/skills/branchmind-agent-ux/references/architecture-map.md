# Architecture mental map (anchors + graph)

The goal is to make “where am I / what matters” cheap across sessions.

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
      { "id": "a:mcp", "title": "MCP adapter", "kind": "component" }
    ]
  },
  "budget_profile": "portal",
  "view": "compact"
}
```

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

## Keep the graph useful (not a diagram dump)

Graph is for:
- ownership and dependency direction,
- critical invariants and boundaries,
- merge conflict discoverability.

Use `graph.query` for bounded scans; use `open` on returned refs for details.

## Code coordinates (until first-class code anchors exist)

When you reference code, store precise receipts:

```text
FILE: crates/mcp/src/main.rs:220-420
COMMIT: <sha>
```

Later these receipts can be migrated into first-class “code anchor” artifacts.

