# Taxonomy (portable): anchors + knowledge keys

Goal: make recall/resume cheap across **many** repos and many years, while preventing knowledge rot.

This file proposes a *portable* baseline taxonomy. Adapt it, but keep it small.

## Anchor design rules (meaning map)

Anchors are meaning coordinates (`a:<slug>`), not file paths.

Rules of thumb:

- Start with **6–12 anchors**. More usually increases noise.
- Prefer **component / boundary** anchors over “micro-feature” anchors.
- If a component is renamed, prefer **alias/merge/rename** flows instead of creating a new anchor.
- Keep anchors stable across refactors; store code receipts as `code:` refs or `FILE:` lines in cards.

## Recommended bootstrap sets

### Minimal (works for most repos)

- `a:core` — domain / invariants
- `a:api` — public API / surface contracts
- `a:storage` — persistence / migrations
- `a:infra` — runtime wiring / deploy / ops
- `a:tests` — test strategy + harnesses
- `a:docs` — docs/contracts/ux doctrine

### Backend service (common)

- `a:core`, `a:api`, `a:storage`
- `a:auth` (if present)
- `a:queue` / `a:jobs` (async work)
- `a:observability` (logs/metrics/traces)
- `a:ci` / `a:release`
- `a:security`, `a:perf` (only if you actually maintain these as first-class)

### Library / SDK

- `a:core` (logic)
- `a:public-api` (stability/compat)
- `a:build` (packaging)
- `a:tests`
- `a:docs`
- `a:compat` (versioning/migrations)

### BranchMind (this repo)

- `a:core`, `a:storage`, `a:mcp`, `a:runner`, `a:graph`, `a:docs`

## Knowledge keys (stable identity)

Knowledge is only useful long-term if keys are searchable and non-bucketed.

Preferred format:

```text
key = <subsystem>-<topic>   # lowercase + dashes, <= ~64 chars
anchor = a:<component>
```

Examples (good):

- `mcp-schema-on-demand`
- `budgets-ladder`
- `jobs-lease-reclaim`
- `storage-tx-atomicity`
- `graph-merge-conflicts`

Anti-patterns (bad):

- `misc`, `general`, `notes`, `todo`, `stuff`
- keys that pack multiple unrelated facts (“bucket keys”)

When unsure:

- use `think.knowledge.key.suggest`
- or run `think.knowledge.lint` weekly and follow consolidation actions

## Card hygiene (prevents rot)

Every durable card must be short and must expire:

```text
Claim: ...
Scope: a:<...> | repo
Apply: ...
Proof: CMD: ... | LINK: ... | FILE: ...
Expiry: YYYY-MM-DD
```

Promotion:

- Add `v:canon` only when reused (≥2) or expensive-to-rediscover.
- Keep hypotheses and in-progress findings as `v:draft`.

## Practical search mindset

Ask: “What will future-me search for at 2am?”

- If the answer is the component name → anchor.
- If the answer is the failure mode / invariant → key.

If a key becomes overloaded:

- split into `subsystem-topic-a` / `subsystem-topic-b`
- update the cards (same anchor, new keys)
- let lint guide consolidation

