# Viewer (read-only, human situational awareness)

BranchMind includes an optional local HTTP viewer:

- loopback only (`127.0.0.1`)
- **read-only** (use MCP portals for all mutations)

## Defaults

- Viewer is enabled by default for stdio/shared modes.
- Default port: `7331`

Open in browser:

```text
http://127.0.0.1:7331
```

Disable:
- CLI: `--no-viewer`
- Env: `BRANCHMIND_VIEWER=0`

Change port:
- CLI: `--viewer-port 7331`
- Env: `BRANCHMIND_VIEWER_PORT=7331`

## What to look at (human UX)

- Task state + steps + proofs
- Knowledge cards (open by CARD id)
- Graph slices (architecture mental map)

Use viewer to understand. Use MCP tools to act.

