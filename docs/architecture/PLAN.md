# Implementation Plan (milestones)

This plan is intentionally staged to keep the system verifiable at every step.

This repository has already shipped past the early milestones: the workspace, storage, and MCP surface are implemented.
The plan below is kept as a *status map* (what exists vs. what remains to harden), to avoid “docs say X, code does Y” drift.

## Milestone 0 — Repo skeleton (**implemented**)

- Cargo workspace with crates: `bm_core`, `bm_storage`, `bm_mcp`
- `bm_mcp` implements MCP stdio JSON-RPC handshake + `tools/list` + `tools/call`
- Minimal contract tests that validate response envelopes and tool list stability

## Milestone 1 — Task domain MVP (execution) (**implemented**)

- `tasks_create` (plan/task) + `tasks_context` (list) + `tasks_edit` (meta) + `tasks_delta` (event stream)
- Revisions (`expected_revision`) and strict targeting semantics
- Focus pointer (`tasks_focus_get/set/clear`) for fast resumption (convenience only)
- `tasks_radar` (one-screen) for a single active target (includes reasoning_ref)
- Step tree primitives: `tasks_decompose` / `tasks_define` / `tasks_note` / `tasks_verify` / `tasks_done`

## Milestone 2 — Reasoning memory MVP (durable log) (**implemented**)

- Notes doc with append-only entries (`notes_commit`)
- Event sink for task mutations (automatic)
- Budgets for all read outputs

## Milestone 3 — Branching + merges (reasoning as VCS) (**implemented**)

- Branch refs + HEAD handling
- Diff and merge strategies + conflict entities discoverable by query
- Export/snapshot tooling for IDE integration

## Milestone 4 — Typed graph + thinking traces (**implemented**)

- Graph apply/query/validate/diff
- Thinking cards/traces (template-driven, low-noise)

## Milestone 5 — Hardening (**implemented**)

- **Contract hardening:** increase coverage so every tool has a “shape lock” (schemas + required fields + typed errors).
- **Budget hardening:** ensure every potentially large response is bounded and degrades predictably (`truncated`, minimal signals, pagination).
- **Correctness hardening:** expand invariant tests (revision gating, conflict lifecycle, strict targeting).
- **Operational hardening:** crash-safety and recovery behavior for the embedded store.
- **Performance hardening:** keep reads O(k) in requested output size; add/adjust indexes as needed.
