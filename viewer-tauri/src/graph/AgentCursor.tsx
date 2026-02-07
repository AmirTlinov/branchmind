/* ── AgentCursor: pulsing ring overlay tracking AI agent focus ── */

import { useMemo } from "react";
import { useSnapshotStore } from "../store/snapshot-store";
import { useGraphStore } from "../store/graph-store";

export function AgentCursor() {
  const focus = useSnapshotStore((s) => s.snapshot?.focus ?? null);
  const view = useGraphStore((s) => s.view);
  const nodesById = useGraphStore((s) => s.nodesById);
  const canvasWidth = useGraphStore((s) => s.canvasWidth);
  const canvasHeight = useGraphStore((s) => s.canvasHeight);

  const pos = useMemo(() => {
    if (!focus?.id) return null;
    const node = nodesById.get(focus.id) ?? (focus.plan_id ? nodesById.get(focus.plan_id) : undefined);
    if (!node) return null;
    const cx = canvasWidth / 2;
    const cy = canvasHeight / 2;
    const sx = cx + (node.x - view.offsetX) * view.scale;
    const sy = cy + (node.y - view.offsetY) * view.scale;
    const margin = 24;
    if (sx < -margin || sx > canvasWidth + margin || sy < -margin || sy > canvasHeight + margin) return null;
    return { x: sx, y: sy, label: focus.title ?? focus.id };
  }, [focus, view, nodesById, canvasWidth, canvasHeight]);

  if (!pos) return null;

  return (
    <div
      className="pointer-events-none absolute z-30 animate-[fadeScale_0.3s_ease-out]"
      style={{ left: pos.x, top: pos.y, transform: "translate(-50%, -50%)" }}
    >
      <div
        className="absolute rounded-full border-2 border-accent animate-[cursorPulse_2s_ease-in-out_infinite]"
        style={{ width: 40, height: 40, left: -20, top: -20 }}
      />
      <div
        className="rounded-full bg-accent shadow-[0_0_12px_var(--color-accent)]"
        style={{ width: 10, height: 10, marginLeft: -5, marginTop: -5 }}
      />
      <div className="absolute left-6 top-1/2 -translate-y-1/2 whitespace-nowrap rounded bg-bg-raised/90 px-1.5 py-0.5 text-[10px] font-medium text-accent backdrop-blur-sm">
        {pos.label}
      </div>
    </div>
  );
}
