# Agent UX Rules (non-negotiable)

This is not a human UI project. The MCP surface *is* the UX.

> ✅ **v1 portal naming:** The server exposes **10 tools** (`status/open/workspace/tasks/jobs/think/graph/vcs/docs/system`).
> Legacy tool names in older docs/tests are **rejected** (`UNKNOWN_TOOL`). Use portals + `cmd`
> (see `docs/contracts/V1_OVERVIEW.md`).

## 1) One-screen defaults

- Provide compact, stable snapshots by default.
- Require explicit flags for large expansions.
- Every unified resume/snapshot must include a **small, versioned handoff capsule** (`capsule`) that stays useful even under aggressive `max_chars` trimming.
- The numeric anti-noise guardrails are fixed in `docs/architecture/NOISE_CONTRACT.md`.

## 1.1) Canonical planning loop (Plan → Slice‑Plans v1)

Plans are executed **one reviewable slice at a time** (avoid “whole repo context + whole repo diff”).

Default loop:

`tasks.slices.propose_next → (edit slice_plan_spec) → tasks.slices.apply → tasks.slice.open/validate → (solo|orchestrate)`

Where:
- **Solo:** `jobs.macro.dispatch.scout` → lead applies patches manually (builder skipped).
- **Orchestrate:** `scout → builder → validator → lead gate → apply`.

## 1.2) Canonical team loop for high-risk slices

For NT+/critical slices, the default orchestration is:

`scout (mini/context) → builder (diff batch) → validator (independent) → lead gate → apply`

UX rules for this loop:

- Scout must stay context-only (no code/diff).
- Before builder, run `jobs.pipeline.context.review` as a cheap fail-closed context gate
  (`pass|need_more|reject` + deterministic next actions).
- Builder and validator exchange only artifact refs (`artifact://...`) + stable slice ids.
- Builder default is strict input mode (no repo/tool discovery); if context is insufficient,
  builder requests bounded `context_request` (max retry loop is capped).
- Gate is always explicit and emits a decision object (`approve|rework|reject`) with reasons/actions.
- Apply is fail-closed (requires `approve`, valid lineage, and revision match).
- `jobs.control.center` should surface the next blocked stage first (`dispatch.validator`, then `pipeline.gate`, then `pipeline.apply`).

## 1.1) Semantic de-duplication: tagged line protocol

Agents should not spend 90% of their context budget on repeated schema boilerplate.

For portal workflows, the server is **context-first** and renders a compact “tagged line” protocol (BM-L1) where:

- the response is **tag-light**: 1 plain content line plus a few tagged utility lines,
- the meaning of each tag is defined once in docs/contracts (and enforced by DX-DoD tests),
- the default portal mode optimizes for “read fast, do the next right action” rather than “dump a full tree”.

Semantics rule:

- The protocol and conventions are explained by a dedicated `help` tool, not repeated in daily portal responses.

BM-L1 (tag-light) structure:

- Plain content lines: stable, skimmable state (e.g. focus/next).
- Untagged command lines: one or more copy/paste-ready next actions, placed after the state line.
  (We intentionally omit a `COMMAND:` prefix to avoid constant semantic noise.)
- Tagged utility lines:
  - `ERROR:` typed error (code + fix)
  - `WARNING:` typed heads-up (code + fix)
- `MORE:` continuation marker (pagination cursor / “more available”)
- `REFERENCE:` is reserved for rare anchors (external evidence, ids) and should not appear by default.
- `WATERMARK:` is reserved for future use; it does not appear in normal BM-L1 outputs.

Invariants:

- Prefer **1** plain content line (keep it stable and skimmable).
- Always provide a command line for the best next action when there is one.
- Keep command lines copy/paste-minimal by omitting stable defaults when the server can supply them safely
  (e.g., omit `checkpoints="gate"` when it is the deterministic default).
- Prefer **one primary** command line. If progressive disclosure is required, allow at most **two** commands:
  first `system op=schema.get` / `system op=cmd.list`, then `<the action>`.
- Keep `ERROR:` lines typed and actionable (code + recovery hint).
- Keep `WARNING:` lines typed and actionable (warning/heads-up + recovery hint).
- Do not teach “switch to json” as a daily habit. If a structured payload is needed, expose it as an explicit
  full-view tool instead (and keep portals context-first).

Transport note:

- When the tool renders BM-L1, the MCP server may return the raw line-protocol text as the tool text output
  (no JSON envelope) to minimize token waste in daily agent workflows.

## 1.2) DX-DoD (flagship guardrail)

This is the small “definition of done” that protects daily AX from slowly regressing back into noise.

If you change portal output formatting or recovery, the change is only acceptable if:

1) Tag-light invariant holds: no JSON envelope, no blank lines, no legacy `WATERMARK:` / `ANSWER:` prefixes in BM-L1.
2) Happy path stays tiny: for daily portals, BM-L1 is **2 lines max**:
   - 1 untagged state line
   - 1 next action command line
3) Errors stay typed and minimal: `ERROR:` plus at most **one** recovery command line (two only for disclosure).
4) Progressive disclosure stays deterministic: if the next action needs discovery/schema, return exactly:
   - `system op=schema.get args={cmd:"..."}`
   - `<action> ...` (must include copy/paste-ready args)
5) Budget signals stay warnings: truncation/clamps must render as `WARNING: BUDGET_*`, never as errors, and should keep
   the output within a single screen.

Canonical smoke scenarios (must stay green in tests):

- `status` (BM-L1) → 1 state line + 1 next action command line.
- `tasks cmd=tasks.macro.start` → `tasks cmd=tasks.snapshot` → both are 2-line “state + command” in BM-L1.
- `tasks cmd=tasks.macro.close.step` without focus in an empty workspace → typed error + portal recovery command (`tasks cmd=tasks.macro.start`).
- `tasks cmd=tasks.macro.close.step` on a proof-required step without proof → typed error (`PROOF_REQUIRED`) + a single portal recovery command (retry with `proof=...`).
- Hidden action recommended (e.g. decompose) → 1 state line + 2 commands (disclosure then action).
- Tiny budget snapshot → `WARNING: BUDGET_*` appears and output remains small.

## 2) Make the next action obvious

If an operation fails due to a fixable precondition, return:

- a typed error,
- a minimal recovery explanation,
- **one best next action** as an executable suggestion.

## 2.1) Recovery UX for hidden operations (progressive disclosure)

Even with a small, stable portal surface, agents will hit unknown `cmd` / unfamiliar `args`.

The MCP adapter must keep recovery **cognitively cheap**:

- Prefer portal equivalents over low-level suggestions (portals are accelerators, not bypasses).
- If the next action needs exact arguments, include `system op=schema.get(args={cmd:"..."})` and a minimal valid call example.
- If the agent is clearly using a wrong/unknown `cmd`, include `system op=cmd.list(args={q:"..."})` (substring search) as the first recovery step.
- Never “double-suggest” the same fix (no prepend + keep the original hidden action): replace when possible.

## 3) Prefer diffs and deltas

- Return diffs/deltas before returning full snapshots.
- Avoid re-sending the same large trees repeatedly.

## 4) Never silently change targets

- Explicit identifiers always win.
- Focus is convenience only; strict targeting must be supported.

## 4.1) Tool surface must be cognitively cheap

The number of tools is part of the UX.

- v1 tool surface is fixed and small: **10 portal tools**.
- Reduce noise via **budget_profile** and **portal_view** (portal→default→audit), not by hiding tools.
- Prefer golden ops for common flows and `op=call + cmd` for the long tail.
- Prefer fewer high-leverage tools + composable macros over many “one-off” wrappers.

Progressive disclosure mechanism:

- `tools/list` always returns the v1 surface (10 tools). `toolset` params are ignored.
- To reduce boilerplate, tool calls may omit `workspace`.
  - The server uses its default workspace (derived deterministically from the repo root, or overridden via
    `--workspace` / `BRANCHMIND_WORKSPACE` (id or absolute path; paths are mapped/bound to ids).
  - Explicit `workspace` always wins.
- To inspect the path→id mapping (transparency), call `workspace op=list` (shows `bound_path`).
- When a default workspace is present, portal outputs should avoid repeating it in “next action” args (keep actions copy/paste-ready but minimal).

## 5) Budgets everywhere

All potentially large responses must be bounded and must expose truncation explicitly.

- If the payload is trimmed beyond normal truncation, return a minimal signal plus `BUDGET_MINIMAL`.
- If `max_chars` is too small for a minimal payload, clamp and emit `BUDGET_MIN_CLAMPED`.

## 6) Scope resolution invariants

- `target` (PLAN/TASK) defines the canonical branch + docs; overrides are not allowed.
- If `target` is absent, explicit `ref`/`branch` wins; otherwise fallback to checkout.
- Doc keys default to `notes` / `graph` / `trace` unless explicitly provided.
- Empty doc identifiers are invalid (explicit error).

## 7) DX pain points → invariants

- Pain: ambiguous scope inputs (`target` + overrides) → Invariant: strict mutual exclusivity.
- Pain: multi-call resumption → Invariant: provide a single bounded context pack.
- Pain: inconsistent errors → Invariant: typed errors + one recovery suggestion.

## 8) “Memory physics” (native-feeling, anti-svalka)

This project treats memory as a deterministic coordinate system plus a stable “HUD”.
The goal is for agents to never spend tokens figuring out “where am I” or “what next”.

### 8.1 Coordinates (always know the address)

- **workspace** = the world / project boundary (hard wall).
- **task focus** = the current mission inside a workspace.
- **step focus (“room”)** = the default working set (usually the first open step of the focused task).
- **anchors** = meaning coordinates (`a:*`) for architecture areas (survive refactors; enable resume-by-meaning).
- **visibility** = anti-noise control (`v:canon` / `v:draft` + pins; drafts are hidden by default).

### 8.2 Lifecycle (prevent the dump)

Every artifact must end up in exactly one lifecycle bucket:

- **frontier**: open questions/hypotheses/tests/blockers (active thinking),
- **anchors**: pinned or published decisions/invariants (stable reference points),
- **cold archive**: closed/unpinned/unscoped history (opt-in retrieval only).

Default “smart” views must bias toward frontier + anchors and keep archive cold.

### 8.3 HUD invariant (one screen → one truth)

Every portal-style response that is meant to be read by an agent (snapshot/resume/watch)
must keep a small stable block that never disappears under budgets:

- **where**: workspace + target + step focus (+ optional meaning anchor)
- **now**: what we’re trying to achieve right now (1–2 lines)
- **why**: top signals (bounded)
- **signal**: one primary reasoning signal (bounded, engine-derived)
- **next**: exactly 1 primary action + 1 backup action (bounded)
- **budget**: what was trimmed and why (explicit)

This is the critical trick: agents learn to “think after reading where/now/next”.

### 8.4 PlanFS-first (planning as durable memory)

Goal: when an agent touches a component, it should be able to resume the right intent fast
from stable files, not from transient session state.

Rules:

- Every plan is an on-disk contract:
  - `docs/plans/<plan_slug>/PLAN.md`
  - `docs/plans/<plan_slug>/SLICE-*.md`
- The planning surface is the default source of truth for multi-agent continuity.
- Use `tasks.*` to track execution and keep these files updated in lockstep; runtime graph stays execution-facing.
- Legacy “knowledge card” patterns are no longer the primary mechanism for context continuity.

## 9) Multi-agent concurrency (leases + audit)

The tool does not assume “many agents editing the same thing” is a good workflow.
Still, the server must protect users from accidental overlap.

Rules:

- Durable memory is **shared-by-default**. Noise control is via `v:draft` and pins, not via per-agent lanes.
- `agent_id` exists primarily for **concurrency semantics** (step leases) and best-effort audit metadata.
- Legacy `lane:agent:*` artifacts may exist in older stores and are treated as draft markers unless promoted (`v:canon`).

## 10) Anti-drift (don’t cross projects)

The server supports running in a “single-tenant workspace” mode to prevent accidental cross-project access:

- A configured default workspace may be optionally locked (mismatched workspace becomes a typed error).
- A configured project guard may be enforced to detect opening a store belonging to a different project.

This is intentionally boring and strict: it is how we keep long-lived memory trustworthy.
