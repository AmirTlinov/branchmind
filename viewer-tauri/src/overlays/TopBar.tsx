/* ── TopBar: project/workspace/lens selectors + runner status ── */

import { useSnapshotStore } from "../store/snapshot-store";
import { useUIStore } from "../store/ui-store";
import { useEventsStore } from "../store/events-store";

export function TopBar() {
  const projects = useSnapshotStore((s) => s.projects);
  const workspaces = useSnapshotStore((s) => s.workspaces);
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);
  const lens = useSnapshotStore((s) => s.lens);
  const setProject = useSnapshotStore((s) => s.setProject);
  const setWorkspace = useSnapshotStore((s) => s.setWorkspace);
  const setLens = useSnapshotStore((s) => s.setLens);
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const runner = snapshot?.runner;
  const sseStatus = useEventsStore((s) => s.status);
  const togglePalette = useUIStore((s) => s.togglePalette);
  const toggleExplorer = useUIStore((s) => s.toggleExplorer);

  const runnerColor =
    runner?.status === "active" || runner?.status === "busy"
      ? "bg-success"
      : runner?.status === "idle"
        ? "bg-accent"
        : runner?.status === "starting"
          ? "bg-warning animate-pulse-dot"
          : "bg-ink-dim";

  const sseColor =
    sseStatus === "connected"
      ? "bg-success"
      : sseStatus === "connecting"
        ? "bg-warning animate-pulse-dot"
        : "bg-danger";

  return (
    <header className="glass glass-edge noise-overlay z-50 flex h-10 shrink-0 items-center gap-2 border-b border-border px-3">
      {/* Explorer toggle */}
      <button
        onClick={toggleExplorer}
        className="shrink-0 rounded p-1 text-ink-dim hover:text-ink hover:bg-border transition-colors"
        title="Toggle Explorer (Ctrl+B)"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
          <rect x="2" y="2" width="12" height="12" rx="1.5" />
          <line x1="6" y1="2" x2="6" y2="14" />
        </svg>
      </button>

      {/* Brand */}
      <span className="text-xs font-semibold text-ink tracking-wide">BranchMind</span>

      <div className="mx-1 h-4 w-px bg-border" />

      {/* Project selector */}
      <select
        value={project ?? ""}
        onChange={(e) => setProject(e.target.value || undefined)}
        className="h-6 rounded border border-border bg-bg-raised px-1.5 text-[11px] text-ink-muted focus:border-accent focus:outline-none"
      >
        <option value="">current</option>
        {projects.map((p) => (
          <option key={p.project_guard} value={p.project_guard}>
            {p.label}{p.is_temp ? " (tmp)" : ""}
          </option>
        ))}
      </select>

      {/* Workspace selector */}
      <select
        value={workspace ?? ""}
        onChange={(e) => setWorkspace(e.target.value || undefined)}
        className="h-6 rounded border border-border bg-bg-raised px-1.5 text-[11px] text-ink-muted focus:border-accent focus:outline-none"
      >
        <option value="">auto</option>
        {workspaces.map((w) => (
          <option key={w.workspace} value={w.workspace}>
            {w.workspace}
          </option>
        ))}
      </select>

      {/* Lens toggle */}
      <div className="flex rounded border border-border overflow-hidden">
        {(["work", "knowledge"] as const).map((l) => (
          <button
            key={l}
            onClick={() => setLens(l)}
            className={`px-2 py-0.5 text-[10px] font-medium uppercase transition-colors ${
              lens === l
                ? "bg-accent/20 text-accent"
                : "text-ink-dim hover:text-ink hover:bg-border"
            }`}
          >
            {l}
          </button>
        ))}
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Search trigger */}
      <button
        onClick={togglePalette}
        className="flex items-center gap-1.5 rounded border border-border px-2 py-0.5 text-[11px] text-ink-dim hover:text-ink hover:border-border-bright transition-colors"
        title="Command Palette (Ctrl+K)"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="5" cy="5" r="3.5" />
          <line x1="7.5" y1="7.5" x2="10.5" y2="10.5" />
        </svg>
        <span className="hidden sm:inline">Search</span>
        <kbd className="hidden sm:inline rounded bg-bg px-1 text-[9px] text-ink-dim">Ctrl+K</kbd>
      </button>

      {/* Status indicators */}
      <div className="flex items-center gap-2 ml-2">
        <div className="flex items-center gap-1" title={`Runner: ${runner?.status ?? "unknown"}`}>
          <span className={`h-2 w-2 rounded-full ${runnerColor}`} />
          <span className="text-[10px] text-ink-dim">{runner?.status ?? "?"}</span>
        </div>
        <div className="flex items-center gap-1" title={`SSE: ${sseStatus}`}>
          <span className={`h-1.5 w-1.5 rounded-full ${sseColor}`} />
        </div>
      </div>
    </header>
  );
}
