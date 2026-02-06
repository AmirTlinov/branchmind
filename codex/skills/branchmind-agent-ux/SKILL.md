---
name: branchmind-agent-ux
description: "Master workflow for using BranchMind as a discipline engine: deep planning, branching reasoning, evolvable knowledge, delegation, and architecture mapping (v1 portals, actions-first)."
---

# BranchMind Agent UX

This skill turns BranchMind into a **discipline engine** for AI agents:

**Observe → Plan → Branch → Decide → Execute → Prove → Close → Remember**

Use it when you want an agent to *consistently* produce high-quality engineering work:
deep planning (edge cases + failure modes), branching idea exploration, durable cross-session memory,
and safe multi-agent delegation.

---

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

---

## Hard rules (AI UX invariants)

1) **Actions-first**
   - Prefer executing server-provided `actions[]` over inventing calls.

2) **Schema-on-demand**
   - If you see `INVALID_INPUT`, do not guess fields:
     call `system.schema.get(cmd=...)` and follow the minimal valid example.

3) **Proof-first**
   - Work is not “done” until proof receipts are attached:
     `CMD:` + `LINK:` (preferred) via `tasks.evidence.capture`, then close the step.

4) **No silent scope drift**
   - Never change targets implicitly. Prefer explicit `task` + `step_id` (or follow actions).

5) **No knowledge junk drawer**
   - Use `(anchor, key)` upserts for knowledge; lint periodically.

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

Templates live in `references/templates.md`.

---

## Where to read next (progressive disclosure)

- Daily workflow + copy/paste calls: `references/workflow.md`
- Planning + branching templates: `references/templates.md`
- Delegation protocol (multi-agent): `references/delegation.md`
- Architecture mental map + anchors + graph: `references/architecture-map.md`
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
