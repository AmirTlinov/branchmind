/* ── CenterArea: tabbed main workspace (Graph | Timeline) ── */

import { GraphShell } from "../graph/GraphShell";
import { useUIStore } from "../store/ui-store";
import { TimelinePanel } from "./TimelinePanel";

function TabButton(props: {
  active: boolean;
  label: string;
  shortcut: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={props.onClick}
      className={[
        "relative rounded px-3 py-1.5 text-[11px] font-medium transition-colors",
        props.active
          ? "bg-border/60 text-ink"
          : "text-ink-dim hover:bg-border/40 hover:text-ink",
      ].join(" ")}
    >
      <span className="flex items-center gap-2">
        {props.label}
        <span className="text-[9px] font-mono text-ink-dim/70">{props.shortcut}</span>
      </span>
    </button>
  );
}

export function CenterArea() {
  const view = useUIStore((s) => s.centerView);
  const setView = useUIStore((s) => s.setCenterView);

  return (
    <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
      <div className="glass glass-edge noise-overlay z-40 flex h-9 shrink-0 items-center gap-1 border-b border-border px-2">
        <TabButton
          active={view === "graph"}
          label="Graph"
          shortcut="⌘1"
          onClick={() => setView("graph")}
        />
        <TabButton
          active={view === "timeline"}
          label="Timeline"
          shortcut="⌘2"
          onClick={() => setView("timeline")}
        />
      </div>
      <div className="relative flex min-h-0 flex-1 overflow-hidden">
        {view === "graph" ? <GraphShell /> : <TimelinePanel />}
      </div>
    </main>
  );
}

