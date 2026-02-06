# Contracts — Skills (v1)

BranchMind ships **built-in skill packs**: deterministic, budget-safe behavior profiles that help
AI agents stay consistent (proof-first, low-noise, recall-first).

Skills are exposed via the `system` portal as a command:

```json
{ "op": "call", "cmd": "system.skill", "args": { "profile": "daily", "max_chars": 2000 } }
```

Schemas are discoverable at runtime:

```text
system op=schema.get args={cmd:"system.skill"}
```

## Input (selected)

- `profile` (string, optional)
  - Allowed: `daily | strict | deep | teamlead`
  - Alias: `research` is accepted as an alias for `deep` (back-compat).
  - Default: `daily`
- `max_chars` (int, optional)
  - Output budget for the returned text.
  - The server clamps it to internal safety caps.

## Output

The command returns a **single text payload** (BM‑L1 line protocol) and sets `line_protocol=true`.

Semantic contract for the text:

- First line identifies the pack:
  - `skill profile=<profile> version=<skill_pack_version>`
- The pack includes a tight “next loop” section that is actionable without extra context.
- If truncated, the payload stays useful under the declared `max_chars`.

## Determinism

Given the same build + `profile` + `max_chars`, the output must be byte-identical.
