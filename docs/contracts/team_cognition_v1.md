[LEGEND]
TEAM_COGNITION = The contract for multi-agent coordination and shared context.

[CONTENT]
Contract: Team cognition v1

## Purpose
Define [TEAM_COGNITION]: coordination rules for multiple agents working on the same codebase.

## Scope
- In scope: lanes, ownership, handoffs, shared artifacts, sync rules.
- Out of scope: org structure and staffing.

## Interface
TODO: define lane model and minimal shared artifact set.

## Errors
TODO: define conflict cases (two owners, stale decisions, missing evidence).

## Examples
```text
lane: "core"
owner: "agent-A"
handoff: "agent-B"
```
