# Noise Contract (anti-drift)

BranchMind is not a UI. The MCP surface **is** the UX.

“Noise” is any output or behavior that increases cognitive load **without changing the next right action**.
Noise kills focus, makes memory untrustworthy, and turns resumption into scrolling.

This document fixes the non-negotiable **anti-noise invariants** and the **test gates** that prevent regressions.

## Design targets

- **One-screen default**: portal-style flows must fit on a single screen in the common case.
- **Stable HUD**: every “resume/snapshot/watch/pack” response must contain a small capsule that preserves orientation and next action under budgets.
- **Cold archive by default**: closed/unpinned/unscoped history is opt-in.
- **Lane isolation by default**: multi-agent drafts must not flood each other; publish is explicit.

## Hard invariants (never break)

1) **Determinism**: same inputs → same outputs (stable ordering).
2) **Read-only intelligence**: “smart” analysis never mutates state; it only proposes actions.
3) **Action discipline**: always return **1 best action + 1 backup** (bounded), never a grab-bag.
4) **No teaching in the hot path**: default responses do not explain schemas; `help` tools do.
5) **Budgets are explicit**: trimming/clamping surfaces as warnings, not errors.

## Numeric invariants (guardrails)

### Portal BM‑L1 (`fmt="lines"`)

- **Happy path (daily)**: **2 lines max**
  - 1 untagged state line
  - 1 next-action command line
  - state line must include `ref=<id>` (stable navigation handle; survives truncation)
- **Progressive disclosure**: **3 lines max**
  - state line
  - `tools/list toolset=...`
  - the hidden action (copy/paste-ready args)
- **Budget warnings**: **≤ 4 lines**
  - warnings must be `WARNING: BUDGET_*`, never `ERROR: BUDGET_*`

### JSON “smart” retrieval

- Default “smart-like” views must keep `engine_actions_limit=2` (primary + backup) unless explicitly overridden.
- Default reads must keep archive cold: show **frontier + anchors** first; history is opt-in (`view="explore"` / explicit flags).

### Low-ceremony lints

- Lints must be **low-noise**: only emit when the caller is clearly attempting a feature and it will behave unexpectedly.
- Lints must be **bounded**: for `trace_step` sequential-meta hints, warnings are capped to a small number.

## Change control (how we avoid regressions)

Any PR that changes default portal output, budget behavior, or recovery must:

- update this contract if it changes an invariant,
- add/extend tests that lock the behavior,
- keep “daily” usage quiet (no new always-on warnings).

## Canonical test gates

These tests are the “noise firewall” and must stay green:

- DX-DoD portal size and disclosure rules (`dx_dod_*`)
- Portal recovery stays minimal (`portal_defaults_*`, `recovery_ux_*`)
- Capsule/HUD survives budgets (`packs_hud_*`)
- Flagship eval gates stay green (`flagship_eval_*`)
- Multi-agent lane defaults stay isolated (`multi_agent_lanes_*`)
