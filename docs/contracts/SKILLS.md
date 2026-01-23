# Contracts — Skills & Behavior Packs (v0)

This document defines the **built-in skills** shipped with BranchMind and exposed via the `skill`
tool.

Skills are **versioned, deterministic behavior packs** intended to shape an AI agent’s workflow
without turning the system into bureaucracy.

Design requirements:

- **Budget-safe**: a skill must remain useful under small `max_chars`.
- **Deterministic**: stable ordering; no random IDs; no timestamps in the payload.
- **Composable**: profiles are focused; deeper detail is available via longer `max_chars` (not extra
  flags).
- **Runnable**: every profile contains a tight loop (“what to do next”) and proof discipline.

---

## Tool: `skill`

Returns a profile-specific behavior pack as **raw text** (BM‑L1 line protocol).

### Input (JSON)

```json
{
  "profile": "daily",
  "max_chars": 2000
}
```

Fields:

- `profile` (optional, string):
  - Allowed: `daily | strict | research | teamlead`
  - Default: `daily`
- `max_chars` (optional, integer):
  - Output budget for the returned text.
  - When present, the server clamps it to internal safety limits.

### Output

The tool returns a **single text payload** (not a structured JSON object) and marks the response as
`line_protocol=true`.

Semantic contract for the text:

- The first line identifies the pack:
  - `skill profile=<profile> version=<skill_pack_version>`
- The pack always includes a “next loop” section that is actionable without extra context.
- If truncated, the payload ends with `...` and still preserves the first-line identity.

### Errors

- `INVALID_INPUT`: arguments not an object, unknown `profile`, or invalid `max_chars` type.

### Determinism

Given the same build + `profile` + `max_chars`, the output must be byte-identical.

---

## Runner integration (non-normative)

The external job runner (e.g. `bm_runner`) may inject a selected skill pack into delegated agent
prompts to make behavior consistent across sessions and terminals.

This does not change server determinism: the runner is out-of-process and opt-in.
