# Agent UX Rules (non-negotiable)

This is not a human UI project. The MCP surface *is* the UX.

## 1) One-screen defaults

- Provide compact, stable snapshots by default.
- Require explicit flags for large expansions.

## 2) Make the next action obvious

If an operation fails due to a fixable precondition, return:

- a typed error,
- a minimal recovery explanation,
- **one best next action** as an executable suggestion.

## 3) Prefer diffs and deltas

- Return diffs/deltas before returning full snapshots.
- Avoid re-sending the same large trees repeatedly.

## 4) Never silently change targets

- Explicit identifiers always win.
- Focus is convenience only; strict targeting must be supported.

## 5) Budgets everywhere

All potentially large responses must be bounded and must expose truncation explicitly.

## 6) Scope resolution invariants

- `target` (PLAN/TASK) defines the canonical branch + docs; overrides are not allowed.
- If `target` is absent, explicit `ref`/`branch` wins; otherwise fallback to checkout.
- Doc keys default to `notes` / `graph` / `trace` unless explicitly provided.
- Empty doc identifiers are invalid (explicit error).

## 7) DX pain points → invariants

- Pain: ambiguous scope inputs (`target` + overrides) → Invariant: strict mutual exclusivity.
- Pain: multi-call resumption → Invariant: provide a single bounded context pack.
- Pain: inconsistent errors → Invariant: typed errors + one recovery suggestion.
