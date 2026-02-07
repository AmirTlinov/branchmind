/* ── Detail Panel: task/plan/knowledge detail view ── */

import { useUIStore } from "../store/ui-store";
import { useDetailData } from "../hooks/useDetailData";
import { PlanDetailView } from "./detail/PlanDetailView";
import { TaskDetailView } from "./detail/TaskDetailView";
import { KnowledgeDetailView } from "./detail/KnowledgeDetailView";

export function DetailPanel() {
  const open = useUIStore((s) => s.detailOpen);
  const selection = useUIStore((s) => s.detailSelection);
  const closeDetail = useUIStore((s) => s.closeDetail);
  const { data, loading, error } = useDetailData();

  if (!open || !selection) return null;

  return (
    <aside className="glass glass-edge noise-overlay z-40 flex h-full w-80 shrink-0 flex-col overflow-hidden border-l border-border">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <span
          className={`rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${
            selection.kind === "plan"
              ? "bg-accent/15 text-accent"
              : selection.kind === "task"
                ? "bg-accent-2/15 text-accent-2"
                : selection.kind === "knowledge"
                  ? "bg-warning/15 text-warning"
                  : "bg-ink-dim/15 text-ink-muted"
          }`}
        >
          {selection.kind}
        </span>
        <span className="min-w-0 flex-1 truncate text-xs text-ink-muted font-mono">
          {selection.id}
        </span>
        <button
          onClick={closeDetail}
          className="shrink-0 text-ink-dim hover:text-ink transition-colors"
          title="Close"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
            <line x1="3" y1="3" x2="11" y2="11" />
            <line x1="11" y1="3" x2="3" y2="11" />
          </svg>
        </button>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto p-4">
        {loading && (
          <div className="flex items-center gap-2 text-xs text-ink-dim">
            <span className="h-3 w-3 animate-spin rounded-full border border-accent/40 border-t-accent" />
            Loading...
          </div>
        )}

        {error && (
          <div className="rounded bg-danger/10 px-2 py-1.5 text-xs text-danger">
            {error}
          </div>
        )}

        {!loading && !error && data?.kind === "plan" && (
          <PlanDetailView detail={data.data} />
        )}

        {!loading && !error && data?.kind === "task" && (
          <TaskDetailView detail={data.data} />
        )}

        {!loading && !error && data?.kind === "knowledge" && (
          <KnowledgeDetailView detail={data.data} />
        )}

        {!loading && !error && !data && (
          <div className="text-xs text-ink-dim">
            Select an item to view details.
          </div>
        )}
      </div>
    </aside>
  );
}
