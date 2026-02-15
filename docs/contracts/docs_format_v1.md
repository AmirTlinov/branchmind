 [LEGEND]

[CONTENT]
Contract: Documentation format v1

## Purpose
Define [[DOC_FORMAT]]: a deterministic documentation shape that humans and agents can parse without guesswork.

## Scope
- In scope: required blocks, token rules, and what the gate validates.
- Out of scope: repository-specific vocabulary (that lives in `LEGEND.md`).

## Interface
Rules:
- Every Markdown doc (except repo-root `AGENTS.md` and repo-root `README.md`) MUST follow [[DOC_FORMAT]].
- The first non-empty line MUST be `[LEGEND]`.
- A document MUST include exactly one content header: `[CONTENT]`.

Token rules:
- Definitions live in `[LEGEND]` as `TOKEN = Meaning`.
- Token names use a constrained vocabulary: uppercase with `_` (recommended) and must be stable.
- References in content:
  - Global tokens (from `LEGEND.md`): `[[TOKEN]]` (recommended; Obsidian graph).
  - Local tokens (per-doc): `[TOKEN]` (validated, stays local).
  - Legacy global form `[TOKEN]` is allowed; `[TOKEN|LEGEND.md]` is an explicit-global legacy form (rarely needed).
- Local tokens must not redefine global tokens.

Contract rules:
- Versioned contracts in `docs/contracts/*_vN.md` must follow `CONTRACT_STANDARD.md`.

## Errors
If a doc violates [DOC_FORMAT] or token rules, the gate fails closed.

## Examples
```md
[LEGEND]
TERM = A locally-defined term for this document.

[CONTENT]
This uses [TERM] without repeating the meaning.
```
