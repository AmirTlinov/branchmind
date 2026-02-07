/* ── Task Detail View ── */

import type { TaskDetail, TaskStep, DocEntry } from "../../api/types";

interface Props { detail: TaskDetail; }

export function TaskDetailView({ detail }: Props) {
  const task = detail.task;
  const steps = detail.steps.items;

  return (
    <div className="flex flex-col gap-3 text-xs">
      <h3 className="text-sm font-semibold text-ink">{task.title || task.id}</h3>

      <div className="flex flex-wrap gap-1.5">
        <span className="rounded bg-accent-2/15 px-1.5 py-0.5 text-[10px] font-medium text-accent-2">
          {task.blocked ? "BLOCKED" : task.status}
        </span>
        {task.priority && (
          <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning">
            {task.priority}
          </span>
        )}
        {task.plan_id && (
          <span className="rounded bg-ink-dim/15 px-1.5 py-0.5 text-[10px] text-ink-muted">
            {task.plan_id}
          </span>
        )}
      </div>

      {task.description && (
        <p className="text-ink-muted leading-relaxed whitespace-pre-wrap">{task.description}</p>
      )}

      {task.context && (
        <div className="rounded bg-bg-raised/50 p-2 font-mono text-[11px] leading-relaxed text-ink-dim whitespace-pre-wrap">
          {task.context}
        </div>
      )}

      {/* Steps */}
      {steps.length > 0 && (
        <div>
          <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">Steps</div>
          <div className="space-y-0.5">
            {steps.map((step) => (
              <StepRow key={step.path} step={step} />
            ))}
          </div>
        </div>
      )}

      {/* Trace tail */}
      {detail.trace_tail && detail.trace_tail.entries.length > 0 && (
        <DocTailSection label="Trace" entries={detail.trace_tail.entries} hasMore={detail.trace_tail.has_more} />
      )}

      {detail.notes_tail && detail.notes_tail.entries.length > 0 && (
        <DocTailSection label="Notes" entries={detail.notes_tail.entries} hasMore={detail.notes_tail.has_more} />
      )}

      <div className="border-t border-border pt-2 text-[10px] text-ink-dim">
        {new Date(task.updated_at_ms).toLocaleString()}
      </div>
    </div>
  );
}

function StepRow({ step }: { step: TaskStep }) {
  const gates = [
    step.criteria_confirmed && "criteria",
    step.tests_confirmed && "tests",
    step.security_confirmed && "security",
    step.perf_confirmed && "perf",
    step.docs_confirmed && "docs",
  ].filter(Boolean);

  return (
    <div className={`flex items-start gap-1.5 rounded px-1.5 py-1 ${step.completed ? "opacity-60" : ""}`}>
      <span className={`mt-0.5 h-3 w-3 shrink-0 rounded-sm border ${
        step.completed ? "bg-success/20 border-success text-success" : step.blocked ? "border-danger" : "border-border"
      } flex items-center justify-center text-[8px]`}>
        {step.completed ? "\u2713" : step.blocked ? "!" : ""}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-xs text-ink">{step.title || step.path}</div>
        {gates.length > 0 && (
          <div className="flex gap-1 mt-0.5">
            {gates.map((g) => (
              <span key={g as string} className="rounded bg-success/10 px-1 text-[8px] text-success">{g}</span>
            ))}
          </div>
        )}
        {step.block_reason && (
          <div className="text-[9px] text-danger mt-0.5">{step.block_reason}</div>
        )}
      </div>
    </div>
  );
}

function DocTailSection({ label, entries, hasMore }: { label: string; entries: DocEntry[]; hasMore: boolean }) {
  return (
    <div>
      <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">{label}</div>
      <div className="space-y-1">
        {entries.slice(-5).map((entry, i) => (
          <div key={i} className="rounded bg-bg-raised/50 p-1.5 font-mono text-[10px] text-ink-muted whitespace-pre-wrap break-words">
            {entry.content.length > 500 ? entry.content.slice(0, 500) + "..." : entry.content}
          </div>
        ))}
        {hasMore && <p className="text-[9px] text-ink-dim italic">...more entries</p>}
      </div>
    </div>
  );
}
