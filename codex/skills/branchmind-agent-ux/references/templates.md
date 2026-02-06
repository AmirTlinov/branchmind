# Templates (strict + deep)

Keep artifacts **short** and **reusable** where possible (cards/branches). Plans are expected to be
**long and decision-complete** (strict: ≥10k chars; deep: ≥15k chars) but must stay high-signal (no filler).

## 0) Step close checklist (proof-first)

Use as the mental checklist before closing any step:

```text
Deliverable: shipped (or explicitly not needed)
Proof: CMD: ... | LINK: ...
Verify: tests run (or explicitly waived with reason)
Edge cases: listed + covered (or explicitly deferred)
Rollback signals: defined (when relevant)
Knowledge: new invariant captured as (anchor,key) card (when discovered)
```

## 1) Strict plan frame (paste into a frame/note)

```text
Goal:
Non-goals:
Constraints:
Approach:
Edge cases:
Failure modes:
Verify (tests/proofs):
Rollback / kill-switch signals:
Stop criteria:
```

## 2) Skeptic preflight (deep)

```text
Counter-hypothesis (how this plan fails):
Falsifier test (what would prove it wrong fast):
Stop criteria (when to abort this approach):
```

## 3) Idea branch (deep; create 2+)

```text
Premise:
Strengths:
Weaknesses:
Failure modes (edge cases):
Cost (time/complexity):
Falsifier test:
```

## 4) Merge decision (deep; required)

```text
Winner:
Why winner:
Why losers fail:
Risks accepted:
Rollback signals:
Next steps:
```

## 5) Knowledge card (CARD format)

```text
Claim: <what is true>
Scope: <where it applies>
Apply: <how to use it>
Proof: CMD: <...> | LINK: <...> | FILE: <...>
Expiry: YYYY-MM-DD
```

### Knowledge key hygiene (stable identity)

```text
Key: <subsystem>-<topic>  (avoid misc/general/notes/todo)
Anchor: a:<component>     (meaning coordinate, not a file path)
Expiry: YYYY-MM-DD        (required; prevents stale truth)
Promotion: add v:canon only when reused>=2 or expensive-to-rediscover
```

## 6) Delegation brief (for jobs)

```text
Goal:
Deliverable:
Constraints:
Stop criteria:
Proof required (CMD/LINK):
Refs:
```

### Manager → delegate steering message (when attention is needed)

```text
Context (refs): TASK-... STEP-... CARD-... code:...
Decision needed:
Options (2-4):
Recommended option:
Stop criteria:
Proof required (CMD/LINK/ref):
```
