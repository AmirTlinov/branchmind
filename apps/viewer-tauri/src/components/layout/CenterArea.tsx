import { useCallback, useEffect, useMemo, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "@/store";
import type { CenterView } from "@/store";
import {
  Activity,
  BookOpen,
  Brain,
  Layout,
  ListChecks,
  Route,
} from "lucide-react";
import { GraphCanvas } from "@/components/graph/GraphCanvas";
import { PlanView } from "@/components/plan/PlanView";
import { DocsView } from "@/components/docs/DocsView";
import { TraceView } from "@/components/trace/TraceView";
import { KnowledgeView } from "@/components/knowledge/KnowledgeView";

const TABS: { id: CenterView; label: string; icon: typeof Activity; shortcut: string }[] = [
  { id: "graph", label: "Architecture", icon: Activity, shortcut: "1" },
  { id: "plan", label: "Plan", icon: ListChecks, shortcut: "2" },
  { id: "notes", label: "Notes", icon: BookOpen, shortcut: "3" },
  { id: "trace", label: "Trace", icon: Route, shortcut: "4" },
  { id: "knowledge", label: "Knowledge", icon: Brain, shortcut: "5" },
];

const TAB_INDEX = Object.fromEntries(TABS.map((t, i) => [t.id, i])) as Record<CenterView, number>;

function ViewTabBar() {
  const active_view = useStore((s) => s.active_view);
  const set_active_view = useStore((s) => s.set_active_view);
  const selected_task = useStore((s) => s.selected_task);
  const steps_summary = useStore((s) => s.steps_summary);

  const handleKeyboard = useCallback(
    (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return;
      const tab = TABS.find((t) => t.shortcut === e.key);
      if (tab) {
        e.preventDefault();
        set_active_view(tab.id);
      }
    },
    [set_active_view],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyboard);
    return () => window.removeEventListener("keydown", handleKeyboard);
  }, [handleKeyboard]);

  const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);

  const progress = useMemo(() => {
    if (!steps_summary) return null;
    const total = Math.max(steps_summary.total_steps, 1);
    const done = steps_summary.completed_steps;
    return { total, done, pct: Math.round((done / total) * 100) };
  }, [steps_summary]);

  return (
    <div className="h-10 px-3 flex items-center bg-[#EBECF0]/60 border-b border-gray-200/50 shrink-0">
      {/* Brand */}
      <div className="flex items-center gap-2 mr-4 pr-4 border-r border-gray-300/30 min-w-0">
        <div className="w-5 h-5 rounded-md bg-gradient-to-br from-gray-800 to-black flex items-center justify-center shrink-0">
          <Layout size={10} className="text-white opacity-90" />
        </div>
        <div className="min-w-0">
          <div className="text-[11px] font-bold tracking-tight text-gray-700 truncate">
            {selected_task ? selected_task.title : "BranchMind"}
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-0.5 flex-1 min-w-0">
        {TABS.map((tab) => {
          const Icon = tab.icon;
          const isActive = active_view === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => set_active_view(tab.id)}
              className={`
                relative flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-medium
                transition-all duration-150 select-none
                ${isActive ? "text-gray-900" : "text-gray-500 hover:text-gray-700 hover:bg-white/40"}
              `}
            >
              {isActive && (
                <motion.div
                  layoutId="active-tab-bg"
                  className="absolute inset-0 bg-white rounded-md shadow-sm shadow-black/5 ring-1 ring-black/[0.03]"
                  transition={{ type: "spring", stiffness: 500, damping: 35 }}
                />
              )}
              <span className="relative flex items-center gap-1.5 min-w-0">
                <Icon size={13} className={isActive ? "text-gray-700" : "text-gray-400"} />
                <span className="truncate">{tab.label}</span>
                <span className="ml-0.5 text-[9px] text-gray-300 font-mono shrink-0">
                  {isMac ? "\u2318" : "^"}
                  {tab.shortcut}
                </span>
              </span>
            </button>
          );
        })}
      </div>

      {/* Status */}
      <div className="flex items-center gap-3 pl-4 border-l border-gray-300/30 shrink-0">
        {progress && (
          <div className="flex items-center gap-2">
            <div className="w-20 h-1.5 bg-white/60 rounded-full overflow-hidden ring-1 ring-black/[0.04]">
              <div
                className="h-full bg-gray-800/80 rounded-full"
                style={{ width: `${progress.pct}%` }}
              />
            </div>
            <span className="text-[10px] text-gray-500 tabular-nums">
              {progress.done}/{progress.total}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

export function CenterArea() {
  const active_view = useStore((s) => s.active_view);
  const prevViewRef = useRef(active_view);
  const direction = TAB_INDEX[active_view] >= TAB_INDEX[prevViewRef.current] ? 1 : -1;

  useEffect(() => {
    prevViewRef.current = active_view;
  }, [active_view]);

  return (
    <div className="w-full h-full min-w-0 flex flex-col bg-[#F7F8FA]">
      <ViewTabBar />
      <div className="flex-1 min-h-0 relative overflow-hidden">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={active_view}
            initial={{ opacity: 0, x: direction * 20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: direction * -20 }}
            transition={{ duration: 0.2, ease: [0.25, 0.1, 0.25, 1] }}
            className="absolute inset-0"
          >
            {active_view === "graph" && <GraphCanvas />}
            {active_view === "plan" && <PlanView />}
            {active_view === "notes" && <DocsView />}
            {active_view === "trace" && <TraceView />}
            {active_view === "knowledge" && <KnowledgeView />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

