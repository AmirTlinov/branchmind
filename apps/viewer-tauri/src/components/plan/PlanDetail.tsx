import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "@/store";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { Markdown } from "@/components/ui/Markdown";
import { PlanSteps } from "./PlanSteps";
import { FileText, ListChecks } from "lucide-react";

/* ------------------------------------------------------------------ */
/*  Tab types                                                          */
/* ------------------------------------------------------------------ */

type Tab = "steps" | "document";

interface TabDef {
  id: Tab;
  label: string;
  icon: typeof FileText;
}

/* ------------------------------------------------------------------ */
/*  PlanDetail                                                         */
/* ------------------------------------------------------------------ */

export function PlanDetail() {
  const selected_plan = useStore((s) => s.selected_plan);
  const steps_summary = useStore((s) => s.steps_summary);

  const hasDocument = !!(selected_plan?.description || selected_plan?.context);

  const tabs: TabDef[] = [
    { id: "steps", label: "Steps", icon: ListChecks },
    ...(hasDocument ? [{ id: "document" as const, label: "Document", icon: FileText }] : []),
  ];

  const [activeTab, setActiveTab] = useState<Tab>("steps");

  // If the plan goes away and we were on document tab, reset
  const effectiveTab = activeTab === "document" && !hasDocument ? "steps" : activeTab;

  /* Progress for header bar */
  const pct = (() => {
    if (!steps_summary) return 0;
    const total = Math.max(steps_summary.total_steps, 1);
    return Math.round((steps_summary.completed_steps / total) * 100);
  })();

  return (
    <div className="w-full h-full overflow-y-auto custom-scrollbar px-6 py-6 space-y-5">
      {/* Plan header (shown only when a plan is loaded) */}
      {selected_plan && (
        <div className="space-y-3">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <div className="text-[17px] font-semibold text-gray-900 leading-snug truncate">
                {selected_plan.title}
              </div>
              <div className="mt-1 flex items-center gap-3">
                <StatusBadge status={selected_plan.status} />
                {steps_summary && (
                  <span className="text-[11px] text-gray-500 tabular-nums">
                    {steps_summary.completed_steps}/{steps_summary.total_steps} steps ({pct}%)
                  </span>
                )}
              </div>
            </div>
          </div>

          {/* Progress bar */}
          {steps_summary && (
            <div className="w-full h-1.5 bg-white/70 ring-1 ring-black/[0.03] rounded-full overflow-hidden">
              <div
                className="h-full bg-gray-900/80 rounded-full transition-all duration-500"
                style={{ width: `${pct}%` }}
              />
            </div>
          )}
        </div>
      )}

      {/* Tab bar (only when there are multiple tabs) */}
      {tabs.length > 1 && (
        <div className="flex items-center gap-1 bg-white/40 ring-1 ring-black/[0.03] rounded-xl p-1">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = effectiveTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={
                  "relative flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[11px] font-medium " +
                  "transition-all duration-150 select-none " +
                  (isActive
                    ? "text-gray-900"
                    : "text-gray-500 hover:text-gray-700 hover:bg-white/40")
                }
              >
                {isActive && (
                  <motion.div
                    layoutId="plan-tab-bg"
                    className="absolute inset-0 bg-white rounded-lg shadow-sm shadow-black/5 ring-1 ring-black/[0.03]"
                    transition={{ type: "spring", stiffness: 500, damping: 35 }}
                  />
                )}
                <span className="relative flex items-center gap-1.5">
                  <Icon size={13} className={isActive ? "text-gray-700" : "text-gray-400"} />
                  {tab.label}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {/* Tab content */}
      <AnimatePresence mode="wait" initial={false}>
        {effectiveTab === "steps" && (
          <motion.div
            key="steps"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.18, ease: [0.25, 0.1, 0.25, 1] }}
          >
            <PlanSteps />
          </motion.div>
        )}

        {effectiveTab === "document" && selected_plan && (
          <motion.div
            key="document"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.18, ease: [0.25, 0.1, 0.25, 1] }}
          >
            <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl p-5 space-y-5">
              {selected_plan.description && (
                <div>
                  <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest mb-3">
                    Description
                  </div>
                  <Markdown text={selected_plan.description} />
                </div>
              )}

              {selected_plan.context && (
                <div>
                  <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest mb-3">
                    Context
                  </div>
                  <Markdown text={selected_plan.context} />
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
