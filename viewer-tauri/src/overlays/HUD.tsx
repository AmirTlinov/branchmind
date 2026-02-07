/* ── HUD: bottom-left graph info overlay ── */

import { useSnapshotStore } from "../store/snapshot-store";
import { useGraphStore } from "../store/graph-store";

export function HUD() {
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const nodes = useGraphStore((s) => s.nodes);
  const lod = useGraphStore((s) => s.lod);
  const view = useGraphStore((s) => s.view);

  const plans = snapshot?.plans ?? [];
  const taskBreakdown = plans.reduce<Record<string, number>>((acc, p) => {
    acc[p.status] = (acc[p.status] || 0) + 1;
    return acc;
  }, {});

  return (
    <div className="pointer-events-none absolute bottom-2 left-2 z-30 flex flex-col gap-1 text-[10px] text-ink-dim">
      <span>nodes: {nodes.length} | LOD: {lod} | zoom: {view.scale.toFixed(2)}</span>
      <span>
        plans: {Object.entries(taskBreakdown).map(([s, c]) => `${s}:${c}`).join(" ")}
      </span>
      <span>
        tasks: {snapshot?.tasks_total ?? 0} | focus: {snapshot?.focus?.id ?? "none"}
      </span>
    </div>
  );
}
