# Local development

## Golden path

```bash
make check
make run-mcp
```

## Storage

- Default store directory: repo-local `.agents/mcp/.branchmind/` (derived from the nearest `.git` root)
- Override: `--storage-dir <path>`

## Workspace safety

- Default workspace: derived from repo root directory name.
- Workspace lock: enabled by default in the zero-arg DX run (`make run-mcp`) and available via `--workspace-lock` / `BRANCHMIND_WORKSPACE_LOCK`.

## Toolsets

- `daily`: minimal portal surface for everyday agent work (default).
- `full`: full parity surface.
- `core`: ultra-minimal tool surface.

Override with `--toolset` or `BRANCHMIND_TOOLSET`.

## Viewer UI

Viewer assets are plain JS/CSS served from `crates/mcp/src/viewer/assets/` with no build step.
