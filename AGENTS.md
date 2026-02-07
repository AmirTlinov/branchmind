# AGENTS.md — BranchMind Rust (Unified Task+Reasoning MCP)

This file is the project map and the operating rules for AI agents developing this repository.

## Mission

Build a **single MCP server** that makes AI agents dramatically better at:

- planning and executing long, complex tasks,
- preserving decisions, evidence, and context across sessions,
- exploring alternatives via branching and merging,
- resuming work fast with low-noise summaries.

No GUI/TUI in the core. MCP-first (stdio). Rust-first. Deterministic.

## Golden files (read first)

- `GOALS.md` — explicit goals and non-goals
- `PHILOSOPHY.md` — philosophy (no implementation hardcoding)
- `docs/GLOSSARY.md` — shared terminology (execution + reasoning)
- `docs/QUICK_START.md` — local developer golden path (run/test/lint)
- `docs/contracts/OVERVIEW.md` — contract entrypoint
- `docs/architecture/ARCHITECTURE.md` — boundaries and dependency direction
- `docs/architecture/PLAN.md` — staged implementation milestones
- `docs/architecture/LEGACY_PITFALLS.md` — what not to repeat from prior implementations
- `REPO_RULES.md` — repository rules and quality gates

## Rules (non-negotiable)

### Determinism & safety

- No outbound network calls (no remote I/O).
- No arbitrary external program execution in `bm_mcp`.
  - Exception: `bm_mcp` may optionally auto-start the first-party delegation runner (`bm_runner`).
    See `docs/contracts/DELEGATION.md`.
- Never log committed artifacts to stdout/stderr outside explicit “read” tools.
- Treat all stored artifacts as potentially sensitive; do not auto-ingest env/config.

### MCP contract discipline

- Contract-first: update `docs/contracts/*` before (or alongside) any behavior change.
- Every tool must have:
  - schema-stable inputs/outputs,
  - explicit budget knobs (`max_bytes`, `max_chars`, `limit`, etc),
  - typed errors with recovery hints.

### “Single organism” integration (core requirement)

- Every mutating `tasks_*` operation must emit a durable event into the reasoning subsystem.
- A task must have a stable reasoning reference (`notes_doc`, `graph_doc`, `trace_doc`) created lazily and persisted.
- Merge conflicts must be discoverable (`status="conflict"` or an equivalent query) and resolvable via an explicit tool.

### Architecture boundaries (hexagonal)

- Domain core contains invariants and pure logic; it must not depend on:
  - MCP transport,
  - storage engines,
  - filesystem/OS I/O.
- Adapters implement ports:
  - MCP adapter: request validation, schema, budgeting, error mapping.
  - Storage adapter: persistence, transactions, indexing.

### Rust + dependency budget

- Prefer std-only components for core logic.
- Any external crate must be justified in `docs/architecture/DEPENDENCIES.md` (why it is needed, alternatives considered, risk).

## Project map (current)

The repo is contract-first and already implemented as a workspace with four crates.

```text
docs/
  contracts/               MCP surface (schemas + semantics)
  architecture/            boundaries, storage, test strategy
crates/
  core/                    pure domain (tasks + reasoning primitives)
  storage/                 persistence adapter (single embedded store)
  mcp/                     MCP server (stdio) adapter
  runner/                  first-party delegation runner (JOB-* worker)
```

## Primary code entrypoints (current)

- MCP server binary: `crates/mcp/src/main.rs`
- Domain core root: `crates/core/src/lib.rs`
- Storage adapter root: `crates/storage/src/lib.rs`
- Runner binary: `crates/runner/src/main.rs`

## “Agent-first” maintenance rules (flagship)

These rules exist to make the codebase **cheap to modify** for AI agents and humans.

### No “god-files”

- Prefer small, focused modules over one large file per subsystem.
- If a module grows past ~800 lines, split it into a directory module (`mod.rs` + submodules).
- Split earlier (even ~300–500 lines) when a file becomes a high-churn hotspot or mixes multiple concerns (DB IO + events + history + budget/serialization).
- Keep `mod.rs` as an orchestration layer: module declarations + shared glue, not “everything at once”.

### Tool implementation shape

- MCP handlers live under `crates/mcp/src/handlers/` and are split by family:
  - `tasks/*` for execution/domain operations.
  - `branchmind/*` for reasoning/memory operations.
- Each handler should follow a stable structure:
  1) parse/validate args (schema discipline),
  2) enforce budgets on outputs (budget discipline),
  3) call storage via request structs (storage API discipline),
  4) map errors → typed MCP errors with recovery hints.

### Tool usage rules (moved)

Rules for *using* BranchMind tools day-to-day (portal set, progressive disclosure, focus/template discipline)
are configured in the local Codex config (`config.toml`) to stay consistent across repositories.
Project-level UX doctrine still lives in `docs/architecture/AGENT_UX.md`.

### Knowledge cards (recall-first, cross-session memory)

BranchMind is designed so an agent can **learn while building** and then “recall” the right knowledge
when touching a subsystem later.

Practical loop:

1) **Before** changing a component: recall what we already know (fast, bounded).
2) If you learn something new: upsert a knowledge card with a stable `(anchor, key)` identity.
3) Periodically lint/consolidate to avoid “knowledge junk drawer”.

Copy/paste examples (v1 portals):

```text
think op=knowledge.recall args={"anchor":"core","limit":12}
```

```text
think op=knowledge.upsert args={"anchor":"core","key":"determinism","card":{"title":"Determinism invariants","text":"Claim: ...\nScope: core\nApply: ...\nProof: CMD: ...\nExpiry: 2027-01-01"}}
```

Notes:
- Knowledge is **versioned**: editing text creates a new `card_id`, while `(anchor,key)` always resolves to the latest.
- `tasks_lint` may emit recall/seed actions when a task is anchored but knowledge is missing (semi-strict guidance).

### Storage API discipline (request structs)

- Any public storage method with “many parameters” must use a request struct.
- Prefer “single input object” calls to keep API stable as fields evolve.
- Keep transaction helpers internal and localized to the store module that owns the operation.

### Contract-first changes

- Any behavior or tool shape change must update `docs/contracts/*` before (or alongside) code changes.
- Add/extend contract tests for new tools or new fields; treat tests as the spec lock.

## Decisions (fixed, unless explicitly changed)

These were previously listed as “blockers”. They are now **decided** to reduce strategic ambiguity for agents and humans.
If we ever change one of these, it must be an explicit decision with contract + architecture updates.

- **Tool naming:** keep two tool families: `tasks_*` (execution) and unprefixed reasoning/memory tools.
  The MCP server name is the namespace, so agents call them as `branchmind.status`, `branchmind.macro_branch_note`, etc.
- **Storage boundary:** a *single embedded store* with transactional atomicity for “task mutation + emitted event” writes.
- **Dependency policy:** “minimal audited deps” in adapters; `bm_core` stays std-only (no “0 deps strict” mandate).

## Practical development workflow (moved)

For humans and CI, use the repo-local golden path commands:

- `make check`
- `make run-mcp`

Codex-specific workflow defaults may also exist in a local `config.toml`.

## Aliases (quick navigation)

- Glossary: `docs/GLOSSARY.md`
- Contracts entrypoint: `docs/contracts/OVERVIEW.md`
- Types/errors: `docs/contracts/TYPES.md`
- Task API: `docs/contracts/TASKS.md`
- Reasoning/memory API: `docs/contracts/MEMORY.md`
- Integration contract: `docs/contracts/INTEGRATION.md`
- Architecture: `docs/architecture/ARCHITECTURE.md`
- Plan: `docs/architecture/PLAN.md`
- UX rules: `docs/architecture/AGENT_UX.md`
