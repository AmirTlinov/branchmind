# Goals — BranchMind Rust (Unified Task+Reasoning MCP)

This project builds a single, agent-first MCP server that combines:

- **Execution control** (tasks, decomposition, checkpoints, progress)
- **Durable reasoning memory** (notes, decisions, evidence, diffs, merges, knowledge graph)

The output is intended to be embedded into an AI-first IDE and to remain usable across long, multi-session engineering efforts without losing context.

## Primary goals (must)

### 1) Agent-first UX (high-leverage)

- Provide **one-screen** high-signal summaries (`radar`/`handoff`-style) that let an agent resume work in ≤2 calls.
- Make every error **actionable**: return a recovery hint and a machine-executable suggestion whenever possible.
- Keep outputs **bounded by default** (budgets everywhere); make “full data” explicit.
- Prefer **deterministic, schema-stable** responses over convenience magic.

### 2) Correct-by-construction task execution

- Represent tasks as explicit structures with **checkpoints** (criteria/tests/security/perf/docs) that gate completion.
- Ensure **no silent mis-target** on writes (explicit target > focus; strict targeting supported).
- Prevent lost updates via **optimistic concurrency** (`revision` / `expected_revision`).
- Preserve a verifiable audit trail for long-running plans (events, evidence, state transitions).

### 3) Durable reasoning memory that strengthens work quality

- Store **explicit artifacts only** (notes, decisions, diffs, evidence); no hidden chain-of-thought.
- Support branching for “what-if” exploration and **explicit merges** with conflict visibility.
- Provide a typed knowledge graph for linking hypotheses ↔ questions ↔ tests ↔ evidence ↔ decisions.

### 4) Single organism (no manual sync)

- `tasks_*` mutations must automatically emit a durable **event stream** into the reasoning subsystem.
- A task must have a stable **reasoning reference** (notes/doc/graph/trace identifiers) that survives restarts.
- Conflicts must be discoverable and resolvable without out-of-band bookkeeping.

### 5) Rust + MCP-only + performance

- MCP-only **core** server (stdio). **No GUI/TUI** in the core.
- Optional **local read-only HTTP viewer** (feature-flagged, loopback-only) for human situational awareness.
- No outbound network calls; the viewer is local-only and strictly read-only.
- Keep the core deterministic and fast; enforce budgets to avoid context blow-ups.
- Prefer a **low-dependency** build and a small runtime footprint suitable for IDE integration.

## Non-goals (explicit)

- Not a model, evaluator, or planner that “thinks for you”.
- Not a general-purpose VCS replacement.
- Not a secrets vault (treat all stored artifacts as potentially sensitive; do not auto-ingest environment state).
- Not a multi-tenant cloud service (initially local-first; concurrency is a scoped problem).

## Success metrics (measurable)

- Resume quality: `radar` returns a correct “Now/Why/Verify/Next” snapshot under a fixed output budget.
- Recovery: common user/agent mistakes (“wrong target”, “stale revision”, “missing checkpoints”) are resolved with ≤1 suggested action.
- Drift: task state and reasoning memory never diverge silently (every mutating task op is reflected as an event).
- Performance: typical operations are sub-10ms on small stores and scale linearly with data size under bounded outputs.

## Quality gates (release blockers)

- Contract tests for MCP schemas + stable response shapes.
- Property/unit tests for core invariants (IDs, revisions, checkpoints, conflict lifecycle).
- Budget enforcement tests (truncation must be explicit and schema-stable).
- No secret leakage via logs; artifacts only returned on explicit read tools.
