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
  if (t === "system") return NotebookPen;
  if (t === "anchor") return Link2;
  if (t === "knowledge") return StickyNote;
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
    case "system":
      return { bg: "bg-slate-100/60", border: "border-slate-300/60", iconBg: "bg-slate-700 text-white" };
    case "anchor":
      return { bg: "bg-indigo-50/45", border: "border-indigo-200/60", iconBg: "bg-indigo-100 text-indigo-800" };
    case "knowledge":
      return { bg: "bg-emerald-50/45", border: "border-emerald-200/60", iconBg: "bg-emerald-100 text-emerald-800" };
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
  if (s === "wip" || s === "open" || s === "active" || s === "in_progress") return "bg-amber-400";
  return "bg-gray-300";
}

export const GraphNode = React.memo(function GraphNode({
  node,
  selected,
  dimmed,
  edgeCount,
  onSelect,
  onDragStart,
  onDragEnd,
  scaleRef,
}: {
  node: SimNode;
  selected: boolean;
  dimmed: boolean;
  edgeCount: number;
  onSelect: (id: string) => void;
  onDragStart: (id: string) => void;
  onDragEnd: () => void;
  scaleRef: React.RefObject<number>;
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
      const scale = scaleRef.current;
      const dx = (e.clientX - startRef.current.x) / scale;
      const dy = (e.clientY - startRef.current.y) / scale;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragRef.current = true;
      node._x = startRef.current.nx + dx;
      node._y = startRef.current.ny + dy;
      node.x.set(node._x);
      node.y.set(node._y);
    },
    [node, scaleRef],
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
          "relative w-full h-full rounded-2xl border backdrop-blur-[6px] cursor-pointer",
          "flex items-center gap-2.5 px-3 select-none transition-[opacity,box-shadow] duration-200",
          styles.bg,
          styles.border,
          selected
            ? "ring-2 ring-gray-900/20 shadow-lg shadow-gray-900/10 border-gray-400/60"
            : "shadow-sm shadow-gray-900/5 hover:ring-2 hover:ring-gray-900/10 hover:shadow-md",
          dimmed && "opacity-20",
        )}
        title={`${node.id}\n${node.node_type}${node.status ? ` • ${node.status}` : ""}`}
      >
        {/* Edge count badge */}
        {edgeCount > 0 && (
          <div className="absolute -top-1.5 -right-1.5 min-w-[18px] h-[18px] rounded-full bg-gray-800 text-white text-[9px] font-bold flex items-center justify-center px-1">
            {edgeCount}
          </div>
        )}

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
          {node.tags && node.tags.length > 0 && (
            <div className="flex items-center gap-1 mt-0.5">
              {node.tags.slice(0, 2).map((t) => (
                <span key={t} className="text-[8px] px-1 py-0.5 rounded bg-gray-100/80 text-gray-500 font-mono truncate max-w-[80px]">
                  {t}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Status dot */}
        <div className={cn("w-2.5 h-2.5 rounded-full shrink-0", dot)} />
      </div>
    </motion.div>
  );
});
