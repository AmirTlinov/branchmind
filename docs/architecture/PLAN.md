# Implementation Plan (milestones)

This plan is intentionally staged to keep the system verifiable at every step.

## Milestone 0 — Repo skeleton (documentation-first)

- Cargo workspace with crates: `bm_core`, `bm_storage`, `bm_mcp`
- `bm_mcp` implements MCP stdio JSON-RPC handshake + `tools/list` + `tools/call`
- Minimal contract tests that validate response envelopes and tool list stability

## Milestone 1 — Task domain MVP (execution)

- `tasks_create` (plan/task) + `tasks_context` (list) + `tasks_edit` (meta) + `tasks_delta` (event stream)
- Revisions (`expected_revision`) and strict targeting semantics
- Focus pointer (`tasks_focus_get/set/clear`) for fast resumption (convenience only)
- `tasks_radar` (one-screen) for a single active target (includes reasoning_ref)

## Milestone 2 — Reasoning memory MVP (durable log)

- Notes doc with append-only entries (`branchmind_notes_commit`)
- Event sink for task mutations (automatic)
- Budgets for all read outputs

## Milestone 3 — Branching + merges (reasoning as VCS)

- Branch refs + HEAD handling
- Diff and merge strategies + conflict entities discoverable by query
- Export/snapshot tooling for IDE integration

## Milestone 4 — Typed graph + thinking traces

- Graph apply/query/validate/diff
- Thinking cards/traces (template-driven, low-noise)

## Milestone 5 — Hardening

- Performance: indexes, pagination, bounded outputs everywhere
- Crash-safety: transactions + recovery
- Security: artifact leakage tests; redaction policy where needed
