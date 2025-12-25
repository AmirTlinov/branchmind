# Legacy Pitfalls (what to avoid)

This repository is inspired by prior task and memory tools, but it must not inherit their architectural mistakes.

## 1) Do not mix UI and core

- No GUI/TUI inside the server.
- The core must be usable headlessly and deterministically.

## 2) Do not rely on agent discipline for consistency

- Manual “remember to sync” steps always fail in long sessions.
- Integration must be automatic and idempotent.

## 3) Do not make focus “magic”

- Focus is convenience, not authority.
- Writes must support strict targeting and revision guards.

## 4) Do not make errors non-actionable

- Validation errors must include “expected shape” + a recipe (`template` or a suggested tool call).

## 5) Do not allow context blowups

- Unbounded logs/diffs/exports will kill agent performance.
- Budgets and explicit truncation are mandatory.

