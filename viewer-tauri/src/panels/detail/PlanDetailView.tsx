/* ── Plan Detail View ── */

import type { PlanDetail, DocEntry } from "../../api/types";

interface Props { detail: PlanDetail; }

export function PlanDetailView({ detail }: Props) {
  const plan = detail.plan;
  return (
    <div className="flex flex-col gap-3 text-xs">
      <h3 className="text-sm font-semibold text-ink">{plan.title || plan.id}</h3>

      <div className="flex flex-wrap gap-1.5">
        <span className="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent">
          {plan.status}
        </span>
        {plan.priority && (
          <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning">
            {plan.priority}
          </span>
        )}
      </div>

      {plan.description && (
        <p className="text-ink-muted leading-relaxed whitespace-pre-wrap">{plan.description}</p>
      )}

      {plan.context && (
        <div className="rounded bg-bg-raised/50 p-2 font-mono text-[11px] leading-relaxed text-ink-dim whitespace-pre-wrap">
          {plan.context}
        </div>
      )}

      {/* Progress */}
      {plan.task_counts && plan.task_counts.total > 0 && (
        <div>
          <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">Progress</div>
          <div className="h-1.5 rounded-full bg-border overflow-hidden">
            <div
              className="h-full bg-accent rounded-full transition-all"
              style={{ width: `${Math.round((plan.task_counts.done / plan.task_counts.total) * 100)}%` }}
            />
          </div>
          <div className="mt-1 flex gap-2 text-[10px] text-ink-dim">
            <span>{plan.task_counts.done}/{plan.task_counts.total} done</span>
            {plan.task_counts.active > 0 && <span>{plan.task_counts.active} active</span>}
            {plan.task_counts.backlog > 0 && <span>{plan.task_counts.backlog} backlog</span>}
          </div>
        </div>
      )}

      <DocTailSection label="Trace" tail={detail.trace_tail} />
      <DocTailSection label="Notes" tail={detail.notes_tail} />

      <div className="border-t border-border pt-2 text-[10px] text-ink-dim">
        {new Date(plan.updated_at_ms).toLocaleString()}
      </div>
    </div>
  );
}

function DocTailSection({ label, tail }: { label: string; tail: { entries: DocEntry[]; has_more: boolean } | null }) {
  if (!tail || tail.entries.length === 0) return null;
  return (
    <div>
      <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">{label}</div>
      <div className="space-y-1">
        {tail.entries.slice(-5).map((entry, i) => (
          <div key={i} className="rounded bg-bg-raised/50 p-1.5 font-mono text-[10px] text-ink-muted whitespace-pre-wrap break-words">
            {entry.content.length > 500 ? entry.content.slice(0, 500) + "..." : entry.content}
          </div>
        ))}
        {tail.has_more && (
          <p className="text-[9px] text-ink-dim italic">...more entries</p>
        )}
      </div>
    </div>
  );
}
