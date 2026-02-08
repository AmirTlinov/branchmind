import { useMemo } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/format";
import {
  AlertTriangle,
  CheckCircle2,
  CircleDashed,
  ListChecks,
  Loader2,
} from "lucide-react";
import type { StepListDto } from "@/api/types";

/* ------------------------------------------------------------------ */
/*  Sub-components                                                     */
/* ------------------------------------------------------------------ */

function Badge({ children }: React.PropsWithChildren) {
  return (
    <span className="px-2 py-1 rounded-lg bg-white/60 ring-1 ring-black/[0.03] text-[10px] text-gray-600 font-mono">
      {children}
    </span>
  );
}

function StepFlags({
  criteria_confirmed,
  tests_confirmed,
  security_confirmed,
  perf_confirmed,
  docs_confirmed,
}: {
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
}) {
  const cell = (ok: boolean, label: string) => (
    <span
      className={cn(
        "px-1.5 py-0.5 rounded-md text-[9px] font-mono",
        ok ? "bg-emerald-50 text-emerald-700" : "bg-gray-50 text-gray-400",
      )}
      title={label}
    >
      {label[0].toUpperCase()}
    </span>
  );
  return (
    <div className="flex items-center gap-1">
      {cell(criteria_confirmed, "criteria")}
      {cell(tests_confirmed, "tests")}
      {cell(security_confirmed, "security")}
      {cell(perf_confirmed, "perf")}
      {cell(docs_confirmed, "docs")}
    </div>
  );
}

function StepIcon({ step, isInProgress }: { step: StepListDto; isInProgress: boolean }) {
  if (step.completed) return <CheckCircle2 size={16} className="text-emerald-500" />;
  if (step.blocked) return <AlertTriangle size={16} className="text-rose-500" />;
  if (isInProgress) return <Loader2 size={16} className="text-blue-500 animate-spin" />;
  return <CircleDashed size={16} className="text-gray-300" />;
}

/* ------------------------------------------------------------------ */
/*  Step detail (inline expansion)                                     */
/* ------------------------------------------------------------------ */

function StepDetail() {
  const selected_step = useStore((s) => s.selected_step);
  if (!selected_step) return null;

  return (
    <motion.div
      layout
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      transition={{ duration: 0.2, ease: [0.25, 0.1, 0.25, 1] }}
      className="overflow-hidden"
    >
      <div className="px-4 pb-4 pt-1 space-y-3 border-t border-gray-200/40 bg-white/30">
        {/* Header */}
        <div>
          <div className="text-[15px] font-semibold text-gray-900">{selected_step.title}</div>
          <div className="text-[10px] text-gray-500 font-mono mt-0.5">
            {selected_step.path} &middot; {selected_step.step_id}
          </div>
        </div>

        {/* Next action */}
        {selected_step.next_action && (
          <div>
            <div className="text-[10px] text-gray-400 uppercase tracking-widest">next action</div>
            <div className="text-[12px] text-gray-800 mt-1">{selected_step.next_action}</div>
          </div>
        )}

        {/* Stop criteria */}
        {selected_step.stop_criteria && (
          <div>
            <div className="text-[10px] text-gray-400 uppercase tracking-widest">stop criteria</div>
            <div className="text-[12px] text-gray-800 mt-1">{selected_step.stop_criteria}</div>
          </div>
        )}

        {/* Success criteria + Tests grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <div>
            <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-2">success criteria</div>
            {selected_step.success_criteria.length === 0 ? (
              <div className="text-[12px] text-gray-500">&mdash;</div>
            ) : (
              <ul className="space-y-1 text-[12px] text-gray-800 leading-relaxed">
                {selected_step.success_criteria.map((c, i) => (
                  <li key={i} className="flex items-start gap-2">
                    <span className="text-gray-300 mt-[2px]">&bull;</span>
                    <span>{c}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div>
            <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-2">tests</div>
            {selected_step.tests.length === 0 ? (
              <div className="text-[12px] text-gray-500">&mdash;</div>
            ) : (
              <ul className="space-y-1 text-[12px] text-gray-800 leading-relaxed">
                {selected_step.tests.map((t, i) => (
                  <li key={i} className="flex items-start gap-2">
                    <span className="text-gray-300 mt-[2px]">&bull;</span>
                    <span>{t}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        {/* Blockers callout */}
        {selected_step.blockers.length > 0 && (
          <div className="rounded-xl bg-rose-50/60 ring-1 ring-rose-200/40 p-3">
            <div className="flex items-center gap-1.5 text-[10px] text-rose-600 uppercase tracking-widest mb-2">
              <AlertTriangle size={12} />
              blockers
            </div>
            <ul className="space-y-1 text-[12px] text-rose-800 leading-relaxed">
              {selected_step.blockers.map((b, i) => (
                <li key={i} className="flex items-start gap-2">
                  <span className="text-rose-300 mt-[2px]">&bull;</span>
                  <span>{b}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </motion.div>
  );
}

/* ------------------------------------------------------------------ */
/*  PlanSteps                                                          */
/* ------------------------------------------------------------------ */

export function PlanSteps() {
  const steps = useStore((s) => s.steps);
  const steps_summary = useStore((s) => s.steps_summary);
  const selected_step_id = useStore((s) => s.selected_step_id);
  const select_step = useStore((s) => s.select_step);

  const pct = useMemo(() => {
    if (!steps_summary) return 0;
    const total = Math.max(steps_summary.total_steps, 1);
    return Math.round((steps_summary.completed_steps / total) * 100);
  }, [steps_summary]);

  const firstOpenId = useMemo(() => {
    const first = steps.find((s) => !s.completed && !s.blocked);
    return first?.step_id ?? null;
  }, [steps]);

  return (
    <div className="space-y-5">
      {/* Header: progress + missing badges */}
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest flex items-center gap-2">
            <ListChecks size={14} />
            Plan steps
          </div>
          {steps_summary && (
            <div className="mt-2 flex items-center gap-3">
              <div className="w-56 h-2 bg-white/70 ring-1 ring-black/[0.03] rounded-full overflow-hidden">
                <div className="h-full bg-gray-900/80 rounded-full transition-all duration-500" style={{ width: `${pct}%` }} />
              </div>
              <div className="text-[12px] text-gray-700 tabular-nums">
                {steps_summary.completed_steps}/{steps_summary.total_steps} ({pct}%)
              </div>
            </div>
          )}
        </div>

        {steps_summary && (
          <div className="flex flex-wrap items-center gap-2 justify-end">
            {steps_summary.missing_criteria > 0 && <Badge>missing C:{steps_summary.missing_criteria}</Badge>}
            {steps_summary.missing_tests > 0 && <Badge>missing T:{steps_summary.missing_tests}</Badge>}
            {steps_summary.missing_security > 0 && <Badge>missing S:{steps_summary.missing_security}</Badge>}
            {steps_summary.missing_perf > 0 && <Badge>missing P:{steps_summary.missing_perf}</Badge>}
            {steps_summary.missing_docs > 0 && <Badge>missing D:{steps_summary.missing_docs}</Badge>}
          </div>
        )}
      </div>

      {/* Step rows */}
      <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl overflow-hidden">
        {steps.map((s, i) => {
          const selected = s.step_id === selected_step_id;
          const isInProgress = s.step_id === firstOpenId;
          return (
            <motion.div
              key={s.step_id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.03, duration: 0.25, ease: [0.25, 0.1, 0.25, 1] }}
              className={cn(
                "border-b border-gray-200/60 last:border-b-0",
                selected && "bg-white/70",
              )}
            >
              <button
                onClick={() => void select_step(s.step_id)}
                className={cn(
                  "w-full px-4 py-3 flex items-center justify-between gap-4 text-left",
                  "hover:bg-white/55 transition-colors",
                )}
                title={s.step_id}
              >
                <div className="flex items-start gap-3 min-w-0">
                  <div className="mt-0.5">
                    <StepIcon step={s} isInProgress={isInProgress} />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="font-mono text-[11px] text-gray-400 shrink-0">{s.path}</span>
                      <span className="text-[13px] text-gray-900 truncate">{s.title}</span>
                    </div>
                    <div className="mt-1 flex items-center gap-2 text-[10px] text-gray-400">
                      <span className="font-mono">{s.step_id}</span>
                      <span>&middot;</span>
                      <span>updated {formatRelative(s.updated_at_ms)}</span>
                      {s.block_reason && (
                        <>
                          <span>&middot;</span>
                          <span className="text-rose-600 truncate">{s.block_reason}</span>
                        </>
                      )}
                    </div>
                  </div>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  <StepFlags
                    criteria_confirmed={s.criteria_confirmed}
                    tests_confirmed={s.tests_confirmed}
                    security_confirmed={s.security_confirmed}
                    perf_confirmed={s.perf_confirmed}
                    docs_confirmed={s.docs_confirmed}
                  />
                </div>
              </button>

              {/* Inline detail expansion */}
              <AnimatePresence>
                {selected && <StepDetail />}
              </AnimatePresence>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
