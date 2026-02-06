---
name: branchmind-agent-ux
description: "10-year daily-driver workflow for BranchMind v1 (portals, actions-first): proof-first execution, branching decisions, knowledge hygiene, anchors, and safe delegation."
---

# BranchMind Agent UX (10‑year daily driver)

BranchMind is not a UI project — the **MCP surface is the UX**. Treat it as a deterministic state machine.

This skill turns BranchMind into a **discipline engine** that stays effective under drift, tight budgets,
and multi‑agent concurrency:

**Observe → Plan → Branch → Decide → Execute → Prove → Close → Remember**

Use it when you want an agent to *consistently* produce high-quality engineering work:
deep planning (edge cases + failure modes), branching idea exploration, durable cross-session memory,
and safe multi-agent delegation.

---

## Mental model (4 layers)

Keep these layers separate in your head and artifacts. It prevents “chat memory” collapse over years.

- **Work (`tasks`)** — deliverables, checkpoints, NextEngine (“what next”), proof receipts.
- **Reasoning (`think` + `docs` + `graph`)** — decisions, alternatives, trace; openable refs, not chat.
- **Knowledge (`think.knowledge.*`)** — reusable invariants as short cards with stable `(anchor,key)`.
- **Meaning map (anchors `a:*`)** — semantic coordinates for recall/resume across refactors.

## Assumptions (required)

- BranchMind MCP server is configured and reachable as the `branchmind` tool namespace.
- v1 surface is **exactly 10 portal tools**:
  `status/open/workspace/tasks/jobs/think/graph/vcs/docs/system`.

---

## Quick start (2 calls, actions-first)

1) **Liveness + NextEngine**
   - Call: `branchmind.status` (Codex: `mcp__branchmind__status`)

2) **Do the next right thing**
   - Execute the first `actions[]` entry **as-is**, OR
   - Copy/paste the first inline command line if the output is BM-L1.

If there is no focus yet:
- Create one via `tasks.macro.start` (see `references/workflow.md`).
  - Default: `template="principal-task"` (strict discipline).
  - Risky/architectural: `template="flagship-task"` (deep discipline: branches + resolved synthesis).

---

## Kernel commands (the “always works” set)

If you remember only these, you can operate BranchMind for a decade without relearning the UX:

- `branchmind.status` — HUD + NextEngine.
- `branchmind.open id=<ref>` — open anything (TASK/STEP/CARD/notes/code/job).
- `branchmind.system op=schema.get args={cmd:"..."}` — recover from `INVALID_INPUT` (never guess).
- `tasks.macro.start` — start with disciplined defaults (small steps, clear verify plan).
- `tasks.evidence.capture` → `tasks.step.close` *(or `tasks.macro.close.step`)* — proof-first close.
- `think.knowledge.recall/upsert/lint` — recall-first + durable memory hygiene.

Everything else is progressive disclosure.

---

## Hard rules (10‑year invariants)

1) **Actions-first**
   - Prefer executing server-provided `actions[]` over inventing calls.

2) **Schema-on-demand**
   - If you see `INVALID_INPUT`, do not guess fields:
     call `system.schema.get(cmd=...)` and follow the minimal valid example.

3) **Proof-first**
   - Work is not “done” until proof receipts are attached:
     `CMD:` + `LINK:` (preferred) via `tasks.evidence.capture`, then close the step.

4) **Refs-first (anti-noise)**
   - Don’t paste large blobs in chat/notes. Store openable artifacts and send refs.
   - Prefer `open <ref>` + small budgets over re-requesting huge views.

5) **Budget ladder**
   - Default: `budget_profile=portal`, `view=compact`.
   - Escalate only when needed: `default/smart` → `audit/audit`.
   - If output truncates: re-open the **specific ref** with a larger budget (never “dump all”).

6) **No silent scope drift**
   - Never change targets implicitly. Prefer explicit `task` + `step_id` (or follow actions).

7) **Single-writer discipline (multi-agent safe)**
   - If you see `STEP_LEASE_HELD`, don’t fight it. Follow recovery actions (release/wait/takeover).

8) **No knowledge junk drawer**
   - Use `(anchor, key)` upserts for knowledge; lint periodically.
   - Every durable card must have an `Expiry:` (prevents “old truth” poisoning).
   - Promote to canon only when reused (≥2) or expensive-to-rediscover.

---

## Discipline policy (trivial / strict / deep)

**Trivial** (soft discipline):
- small change, obvious, low risk.
- still do Observe → Execute → Prove → Close.

**Strict** (default for “real” work):
- requires a plan frame + edge cases/failure modes + verification plan **before** implementation.

**Deep** (for architecture/security/migrations/high-unknown work):
- strict requirements, plus branching alternatives + falsifier attempts + merge decision.

> When in doubt, choose **Strict**.

### Mechanical enforcement (recommended)

BranchMind can **enforce** discipline on step close when the task sets:

- `reasoning_mode=strict` — requires hypotheses/tests/counter-position before closing.
- `reasoning_mode=deep` — strict superset; additionally requires a **resolved decision** per step.

If the gate blocks, follow the portal-first suggestions (`think.card`, `think.playbook`, …).
Escape hatch (macro flow only): `tasks.macro.close.step override={reason,risk}` emits
`WARNING: REASONING_OVERRIDE_APPLIED` and records an explicit debt note.

Templates live in `references/templates.md`.

---

## Cadence (how this stays effective for years)

- **Daily**: `status` → do next action → attach proof → close step → upsert knowledge when you learn.
- **Weekly**: `think.knowledge.lint` (follow actions) to prevent key drift / duplication.
- **After upgrades / weird behavior**: check `build=<fingerprint>` in `status`; use `system.daemon.restart` if stale.

---

## Where to read next (progressive disclosure)

- Daily workflow + copy/paste calls: `references/workflow.md`
- Planning + branching templates: `references/templates.md`
- Delegation protocol (multi-agent): `references/delegation.md`
- Teamlead protocol (manage 3–10 agents): `references/teamlead.md`
- Architecture mental map + anchors + graph: `references/architecture-map.md`
- Portable taxonomy (anchors + knowledge keys): `references/taxonomy.md`
- Viewer (read-only) navigation: `references/viewer.md`
- Troubleshooting: `references/troubleshooting.md`

---

## Install (for users)

Install into Codex using the system skill-installer:

```bash
python3 ~/.codex/skills/.system/skill-installer/scripts/install-skill-from-github.py \
  --repo AmirTlinov/branchmind \
  --path codex/skills/branchmind-agent-ux \
  --ref main
```

After installing: **restart Codex** to pick up new skills.
