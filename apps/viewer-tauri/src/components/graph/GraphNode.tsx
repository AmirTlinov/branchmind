import React, { useCallback, useMemo, useRef } from "react";
import { motion } from "motion/react";
import type { SimNode } from "./useForceLayout";
import {
  CheckCircle2,
  FlaskConical,
  HelpCircle,
  Lightbulb,
  Link2,
  ListChecks,
  NotebookPen,
  RefreshCcw,
  SquareDashedKanban,
  StickyNote,
} from "lucide-react";
import { cn } from "@/lib/cn";

const NODE_W = 240;
const NODE_H = 70;

function resolveIcon(nodeType: string) {
  const t = nodeType.toLowerCase();
  if (t === "task") return SquareDashedKanban;
  if (t === "step") return ListChecks;
  if (t === "note") return StickyNote;
  if (t === "decision") return CheckCircle2;
  if (t === "evidence") return Link2;
  if (t === "hypothesis") return Lightbulb;
  if (t === "test") return FlaskConical;
  if (t === "frame") return NotebookPen;
  if (t === "question") return HelpCircle;
  if (t === "update") return RefreshCcw;
  return Lightbulb;
}

function styleFor(nodeType: string) {
  const t = nodeType.toLowerCase();
  switch (t) {
    case "task":
      return { bg: "bg-white/60", border: "border-gray-200/60", iconBg: "bg-gray-900 text-white" };
    case "step":
      return { bg: "bg-white/55", border: "border-gray-200/60", iconBg: "bg-gray-100 text-gray-700" };
    case "note":
      return { bg: "bg-emerald-50/40", border: "border-emerald-200/50", iconBg: "bg-emerald-100 text-emerald-800" };
    case "decision":
      return { bg: "bg-indigo-50/40", border: "border-indigo-200/50", iconBg: "bg-indigo-100 text-indigo-800" };
    case "evidence":
      return { bg: "bg-amber-50/40", border: "border-amber-200/50", iconBg: "bg-amber-100 text-amber-800" };
    case "hypothesis":
      return { bg: "bg-sky-50/40", border: "border-sky-200/50", iconBg: "bg-sky-100 text-sky-800" };
    case "test":
      return { bg: "bg-purple-50/40", border: "border-purple-200/50", iconBg: "bg-purple-100 text-purple-800" };
    case "frame":
      return { bg: "bg-gray-50/40", border: "border-gray-200/60", iconBg: "bg-gray-100 text-gray-700" };
    case "question":
      return { bg: "bg-rose-50/35", border: "border-rose-200/50", iconBg: "bg-rose-100 text-rose-800" };
    default:
      return { bg: "bg-white/55", border: "border-gray-200/60", iconBg: "bg-gray-100 text-gray-700" };
  }
}

function statusColor(status?: string | null) {
  const s = (status || "").toLowerCase();
  if (!s) return "bg-gray-300";
  if (s === "done" || s === "closed" || s === "ok") return "bg-emerald-400";
  if (s === "blocked" || s === "conflict") return "bg-rose-400";
  if (s === "wip" || s === "open") return "bg-amber-400";
  return "bg-gray-300";
}

export const GraphNode = React.memo(function GraphNode({
  node,
  selected,
  dimmed,
  onSelect,
  onDragStart,
  onDragEnd,
  scale,
}: {
  node: SimNode;
  selected: boolean;
  dimmed: boolean;
  onSelect: (id: string) => void;
  onDragStart: (id: string) => void;
  onDragEnd: () => void;
  scale: number;
}) {
  const dragRef = useRef(false);
  const startRef = useRef({ x: 0, y: 0, nx: 0, ny: 0 });

  const Icon = useMemo(() => resolveIcon(node.node_type), [node.node_type]);
  const styles = useMemo(() => styleFor(node.node_type), [node.node_type]);
  const dot = statusColor(node.status);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.stopPropagation();
      e.currentTarget.setPointerCapture(e.pointerId);
      dragRef.current = false;
      startRef.current = { x: e.clientX, y: e.clientY, nx: node._x, ny: node._y };
      onDragStart(node.id);
    },
    [node, onDragStart],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      const dx = (e.clientX - startRef.current.x) / scale;
      const dy = (e.clientY - startRef.current.y) / scale;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragRef.current = true;
      node._x = startRef.current.nx + dx;
      node._y = startRef.current.ny + dy;
      node.x.set(node._x);
      node.y.set(node._y);
    },
    [node, scale],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      e.currentTarget.releasePointerCapture(e.pointerId);
      onDragEnd();
      if (!dragRef.current) onSelect(node.id);
    },
    [node.id, onDragEnd, onSelect],
  );

  return (
    <motion.div
      style={{
        x: node.x,
        y: node.y,
        width: NODE_W,
        height: NODE_H,
        marginLeft: -NODE_W / 2,
        marginTop: -NODE_H / 2,
      }}
      className="absolute top-0 left-0 will-change-transform"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      data-no-pan
    >
      <div
        className={cn(
          "w-full h-full rounded-2xl border backdrop-blur-xl cursor-pointer",
          "flex items-center gap-2.5 px-3 select-none transition-all duration-200",
          styles.bg,
          styles.border,
          selected
            ? "ring-2 ring-gray-900/20 shadow-lg shadow-gray-900/10 border-gray-400/60"
            : "shadow-sm shadow-gray-900/5",
          dimmed && "opacity-20",
        )}
        title={`${node.id}\n${node.node_type}${node.status ? ` • ${node.status}` : ""}`}
      >
        {/* Icon */}
        <div className={cn("w-8 h-8 rounded-xl flex items-center justify-center shrink-0", styles.iconBg)}>
          <Icon size={16} />
        </div>

        {/* Label */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[12px] text-gray-900 truncate">{node.label}</span>
          </div>
          <div className="text-[10px] text-gray-400 font-mono truncate mt-0.5">
            {node.node_type} • {node.id}
          </div>
        </div>

        {/* Status dot */}
        <div className={cn("w-2.5 h-2.5 rounded-full shrink-0", dot)} />
      </div>
    </motion.div>
  );
});
