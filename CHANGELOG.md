# Changelog

## 2.0.0 — 2026-02-15

### Breaking
- Removed Knowledge Cards command surface:
  - `think.add.knowledge`
  - `think.knowledge.*`
- Removed `promote_to_knowledge`-style behavior from notes/memory flows.
- No compatibility flag/legacy mode is provided.

### Added
- PlanFS v1 task bridge:
  - `tasks.planfs.init`
  - `tasks.planfs.export`
  - `tasks.planfs.import`
- `doc_kind=plan_spec` support in `docs.show/diff/merge`.
- Delegation target resolution from PlanFS `target_ref` with bounded slice excerpts.

### Changed
- `system.cmd.list` and `system.schema.list` default to `mode=golden`; use `mode=all` for full registry.
- Release guidance: use repo-local skills (`.agents/skills/**`) + PlanFS docs as durable operational memory.
