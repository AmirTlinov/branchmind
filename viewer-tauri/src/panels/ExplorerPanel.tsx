/* ── Explorer Panel: sidebar with summary, plans, tasks ── */

import { useSnapshotStore } from "../store/snapshot-store";
import { useUIStore } from "../store/ui-store";
import type { PlanSummary, TaskSummary } from "../api/types";

function statusColor(status: string): string {
  switch (status) {
    case "ACTIVE": case "IN_PROGRESS": return "text-accent";
    case "DONE": case "COMPLETED": return "text-success";
    case "BLOCKED": return "text-danger";
    case "PARKED": return "text-warning";
    default: return "text-ink-dim";
  }
}

function priorityBadge(priority: string | null): string {
  if (!priority) return "";
  if (priority === "CRITICAL") return "bg-danger/20 text-danger";
  if (priority === "HIGH") return "bg-warning/20 text-warning";
  return "bg-ink-dim/20 text-ink-dim";
}

export function ExplorerPanel() {
  const open = useUIStore((s) => s.explorerOpen);
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const knowledgeSnapshot = useSnapshotStore((s) => s.knowledgeSnapshot);
  const lens = useSnapshotStore((s) => s.lens);
  const selectedPlanId = useSnapshotStore((s) => s.selectedPlanId);
  const setSelectedPlanId = useSnapshotStore((s) => s.setSelectedPlanId);
  const openDetail = useUIStore((s) => s.openDetail);
  const togglePalette = useUIStore((s) => s.togglePalette);

  if (!open) return null;

  if (lens === "knowledge" && knowledgeSnapshot) {
    return (
      <aside className="glass glass-edge noise-overlay z-40 flex h-full w-64 shrink-0 flex-col overflow-hidden border-r border-border">
        <div className="border-b border-border px-3 py-2">
          <h2 className="text-xs font-semibold text-ink uppercase tracking-wide">Knowledge</h2>
          <button
            onClick={togglePalette}
            className="mt-2 w-full rounded border border-border bg-bg px-2 py-1 text-left text-[10px] text-ink-dim hover:bg-border/30 transition-colors"
          >
            Search… <span className="float-right font-mono opacity-70">Ctrl+K</span>
          </button>
          <p className="text-[10px] text-ink-dim mt-0.5">
            {knowledgeSnapshot.anchors_total} anchors, {knowledgeSnapshot.keys_total} keys
          </p>
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          {knowledgeSnapshot.keys.map((k) => (
            <button
              key={k.card_id}
              onClick={() => openDetail({ kind: "knowledge", id: k.card_id })}
              className="w-full text-left rounded px-2 py-1.5 hover:bg-border/50 transition-colors mb-0.5"
            >
              <div className="truncate text-xs text-ink">{k.title || k.key}</div>
              <div className="flex items-center gap-1 mt-0.5">
                <span className="text-[9px] text-accent-2">{k.anchor}</span>
                <span className="text-[9px] text-ink-dim">{k.key}</span>
              </div>
            </button>
          ))}
        </div>
      </aside>
    );
  }

  const plans = snapshot?.plans ?? [];
  const tasks = snapshot?.tasks ?? [];
  const focusPlanId = selectedPlanId ?? snapshot?.primary_plan_id;
  const planTasks = focusPlanId
    ? tasks.filter((t) => t.plan_id === focusPlanId)
    : tasks;

  return (
    <aside className="glass glass-edge noise-overlay z-40 flex h-full w-64 shrink-0 flex-col overflow-hidden border-r border-border">
      {/* Summary */}
      <div className="border-b border-border px-3 py-2">
        <h2 className="text-xs font-semibold text-ink uppercase tracking-wide">Explorer</h2>
        <button
          onClick={togglePalette}
          className="mt-2 w-full rounded border border-border bg-bg px-2 py-1 text-left text-[10px] text-ink-dim hover:bg-border/30 transition-colors"
        >
          Search… <span className="float-right font-mono opacity-70">Ctrl+K</span>
        </button>
        <div className="mt-1 flex gap-3 text-[10px] text-ink-dim">
          <span>{snapshot?.plans_total ?? 0} plans</span>
          <span>{snapshot?.tasks_total ?? 0} tasks</span>
        </div>
      </div>

      {/* Plans list */}
      <div className="border-b border-border">
        <div className="px-3 py-1.5 text-[10px] font-medium uppercase text-ink-dim">Plans</div>
        <div className="max-h-48 overflow-y-auto px-1 pb-1">
          {plans.map((plan) => (
            <PlanRow
              key={plan.id}
              plan={plan}
              selected={plan.id === focusPlanId}
              onSelect={() => {
                setSelectedPlanId(plan.id === focusPlanId ? null : plan.id);
              }}
              onDetail={() => openDetail({ kind: "plan", id: plan.id })}
            />
          ))}
          {plans.length === 0 && (
            <p className="px-2 py-2 text-[10px] text-ink-dim">No plans</p>
          )}
        </div>
      </div>

      {/* Tasks list */}
      <div className="flex-1 overflow-y-auto">
        <div className="px-3 py-1.5 text-[10px] font-medium uppercase text-ink-dim">
          Tasks {focusPlanId && `(${focusPlanId})`}
        </div>
        <div className="px-1 pb-1">
          {planTasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              onClick={() => openDetail({ kind: "task", id: task.id })}
            />
          ))}
          {planTasks.length === 0 && (
            <p className="px-2 py-2 text-[10px] text-ink-dim">No tasks</p>
          )}
        </div>
      </div>
    </aside>
  );
}

function PlanRow({
  plan,
  selected,
  onSelect,
  onDetail,
}: {
  plan: PlanSummary;
  selected: boolean;
  onSelect: () => void;
  onDetail: () => void;
}) {
  const tc = plan.task_counts;
  const progress = tc.total > 0 ? Math.round((tc.done / tc.total) * 100) : 0;

  return (
    <div
      className={`group flex items-center gap-1 rounded px-2 py-1 cursor-pointer transition-colors ${
        selected ? "bg-accent/10" : "hover:bg-border/50"
      }`}
      onClick={onSelect}
    >
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-ink">{plan.title || plan.id}</div>
        <div className="flex items-center gap-1.5 mt-0.5">
          <span className={`text-[9px] ${statusColor(plan.status)}`}>{plan.status}</span>
          {tc.total > 0 && (
            <span className="text-[9px] text-ink-dim">{progress}%</span>
          )}
        </div>
      </div>
      <button
        onClick={(e) => { e.stopPropagation(); onDetail(); }}
        className="shrink-0 opacity-0 group-hover:opacity-100 rounded p-0.5 hover:bg-border transition-all text-ink-dim hover:text-ink"
        title="View details"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5">
          <polyline points="4,2 8,6 4,10" />
        </svg>
      </button>
    </div>
  );
}

function TaskRow({
  task,
  onClick,
}: {
  task: TaskSummary;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="w-full text-left rounded px-2 py-1 hover:bg-border/50 transition-colors"
    >
      <div className="flex items-center gap-1">
        <span className={`text-[9px] ${statusColor(task.status)}`}>
          {task.blocked ? "BLOCKED" : task.status}
        </span>
        {task.priority && (
          <span className={`rounded px-1 text-[8px] font-medium ${priorityBadge(task.priority)}`}>
            {task.priority}
          </span>
        )}
      </div>
      <div className="truncate text-xs text-ink mt-0.5">{task.title || task.id}</div>
    </button>
  );
}
