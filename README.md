# BranchMind — durable working memory + execution cockpit for AI agents (MCP server)

BranchMind is a **Rust-first, MCP-only** server that helps AI agents execute long, complex work
without losing context, proof, or decisions. Think “mission control” for tasks + “versioned brain”
for reasoning — **low noise, high discipline, deterministic**.

## Why it exists

Agents drift. Evidence gets lost. Multi‑session work becomes a mess.
BranchMind makes progress **traceable**, **resumable**, and **auditable** while keeping the UX
lean enough for daily use.

## Who it’s for

- Agent builders who need **durable memory** + **proof-first execution**.
- Teams running long tasks that span sessions and need **zero‑drift handoffs**.
- Developers who want **deterministic, local‑only** tooling with clear contracts.

## What you get

- **Task execution control tower**: checkpoints, progress radar, “next action” guidance.
- **Versioned reasoning memory**: notes, diffs, merges, graphs, thinking traces.
- **Proof‑first gates**: close steps with real receipts, not narrative.
- **Low‑noise daily portal**: minimal tool surface, progressive disclosure.
- **Delegation jobs** via `bm_runner` (out‑of‑process) with a fail-closed pipeline:
  scout → builder → validator → gate → apply.

## Concrete benefits (what users actually feel)

**For AI agents**
- **Knowledge that sticks**: versioned memory you can recall, update, and lint (no “lost context”).
- **Planning with control**: explicit steps, checkpoints, and next‑action guidance.
- **Better reasoning**: hypotheses → tests → evidence → decisions, not just chat.
- **Artifacts you can trust**: proofs, logs, and refs are first‑class.
- **Safe exploration**: branch/merge reasoning without losing the main thread.
- **Parallel execution**: delegate jobs without breaking determinism.

**For users and teams**
- **Fewer regressions**: “DONE” means proven, not assumed.
- **Faster resumes**: restart a week later and continue at the exact step.
- **Auditable history**: who did what, why, with evidence.
- **Lower coordination cost**: shared memory and structured handoffs.
- **Predictable behavior**: local‑only, deterministic, contract‑first.

## How it works (mental model)

1. `status` gives the **next action**.
2. `tasks.snapshot` is your **compass**; `open <ref>` is your **zoom**.
3. For multi-agent slices, run scout → builder → validator, then gate before apply.
4. Apply only from an approved gate decision (fail-closed by default).
5. Close steps with `tasks.macro.close.step` + `proof_input`  
   (URL/CMD/path → LINK/CMD/FILE; NOTE doesn’t satisfy proof).
6. Persist learning with `think.knowledge.upsert` and keep it clean via `think.knowledge.lint`.

## Quick start (from source)

```bash
make check
make run-mcp
```

## Add to your MCP client

Example config (OpenCode-style):

```json
{
  "mcp": {
    "branchmind": {
      "type": "local",
      "command": ["/abs/path/to/bm_mcp"],
      "enabled": true,
      "timeout": 30000
    }
  }
}
```

Notes:
- Build/install both binaries so runner autostart works:
  `bm_mcp` and `bm_runner` should sit in the same directory (or `bm_runner` in `PATH`).

## Runtime flags (selected)

- `--storage-dir <path>` — embedded store directory.
- `--workspace <id|path>` — default workspace id (or an absolute path that will be mapped/bound to an id).
- `--agent-id <id|auto>` — default actor id (stored once, reused across restarts).
- `--toolset daily|full|core` — controls default UX/formatting (e.g. BM‑L1 line mode vs full JSON). The v1 tool surface (`tools/list`) remains the fixed 10 portals.
- `--shared` / `--daemon` — shared local daemon modes.

Full list: `bm_mcp --help`.

## Repository map

```text
crates/
  core/      pure domain (tasks + reasoning primitives)
  storage/   persistence adapter (single embedded store)
  mcp/       MCP server (stdio) adapter
  runner/    delegation runner (JOB-* worker)
docs/
  contracts/     MCP surface (schemas + semantics)
  architecture/  boundaries, storage, test strategy
```

## Start here (docs)

- `GOALS.md` — what “done” means
- `PHILOSOPHY.md` — guiding principles
- `AGENTS.md` — development rules for AI agents
- `docs/contracts/OVERVIEW.md` — contract entrypoint
- `docs/QUICK_START.md` — developer golden path
