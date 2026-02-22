# Dependency Policy (current)

## Rules

- `bm_core` must stay std-only.
- Adapter crates may use minimal audited dependencies.
- Any new dependency requires explicit justification.

## Current dependencies

### `bm_core`

- no external crates

### `bm_storage`

- `rusqlite` — embedded transactional store

### `bm_mcp`

- `serde` / `serde_json` — MCP JSON parsing/serialization
- `sha2` — deterministic merge id hashing
- `time` — RFC3339 timestamps in responses
- `nix` (unix) — fd polling for stable shared mode behavior
