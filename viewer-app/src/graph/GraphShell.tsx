/* ── GraphShell: main graph area with canvas + overlays ── */

import { useRef, useEffect, useCallback } from "react";
import { useSnapshotStore } from "../store/snapshot-store";
import { useGraphStore } from "../store/graph-store";
import { useUIStore } from "../store/ui-store";
import { buildGraphModel } from "./model";
import { drawGraph } from "./draw";
import { AgentCursor } from "./AgentCursor";

export function GraphShell() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const lens = useSnapshotStore((s) => s.lens);
  const selectedPlanId = useSnapshotStore((s) => s.selectedPlanId);

  const nodes = useGraphStore((s) => s.nodes);
  const edges = useGraphStore((s) => s.edges);
  const view = useGraphStore((s) => s.view);
  const lod = useGraphStore((s) => s.lod);
  const hoverId = useGraphStore((s) => s.hoverId);
  const setNodesAndEdges = useGraphStore((s) => s.setNodesAndEdges);
  const updateView = useGraphStore((s) => s.updateView);
  const setLod = useGraphStore((s) => s.setLod);
  const setHoverId = useGraphStore((s) => s.setHoverId);
  const updateCanvas = useGraphStore((s) => s.updateCanvas);
  const canvasWidth = useGraphStore((s) => s.canvasWidth);
  const canvasHeight = useGraphStore((s) => s.canvasHeight);

  const openDetail = useUIStore((s) => s.openDetail);

  // Build graph model from snapshot
  useEffect(() => {
    if (!snapshot) return;
    const { nodes: n, edges: e } = buildGraphModel(snapshot, selectedPlanId, lens);
    setNodesAndEdges(n, e);
  }, [snapshot, selectedPlanId, lens, setNodesAndEdges]);

  // Resize observer
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      updateCanvas(width, height);
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [updateCanvas]);

  // Draw
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasWidth * dpr;
    canvas.height = canvasHeight * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawGraph(ctx, nodes, edges, view, lod, hoverId, canvasWidth, canvasHeight);
  }, [nodes, edges, view, lod, hoverId, canvasWidth, canvasHeight]);

  // LOD from zoom
  useEffect(() => {
    if (view.scale < 0.9) setLod("overview");
    else if (view.scale < 1.35) setLod("clusters");
    else setLod("tasks");
  }, [view.scale, setLod]);

  // Pan/zoom handlers
  const dragRef = useRef<{ startX: number; startY: number; ofsX: number; ofsY: number } | null>(null);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const factor = e.deltaY > 0 ? 0.9 : 1.1;
    const newScale = Math.max(0.1, Math.min(10, view.scale * factor));
    updateView({ scale: newScale });
  }, [view.scale, updateView]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    dragRef.current = { startX: e.clientX, startY: e.clientY, ofsX: view.offsetX, ofsY: view.offsetY };
  }, [view.offsetX, view.offsetY]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragRef.current) {
      // Hit test for hover
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const cx = canvasWidth / 2;
      const cy = canvasHeight / 2;
      let found: string | null = null;
      for (const node of nodes) {
        const sx = cx + (node.x - view.offsetX) * view.scale;
        const sy = cy + (node.y - view.offsetY) * view.scale;
        const sr = node.radius * view.scale;
        const dx = mx - sx;
        const dy = my - sy;
        if (dx * dx + dy * dy < sr * sr) {
          found = node.id;
          break;
        }
      }
      setHoverId(found);
      return;
    }
    const dx = e.clientX - dragRef.current.startX;
    const dy = e.clientY - dragRef.current.startY;
    updateView({
      offsetX: dragRef.current.ofsX - dx / view.scale,
      offsetY: dragRef.current.ofsY - dy / view.scale,
    });
  }, [nodes, view, canvasWidth, canvasHeight, setHoverId, updateView]);

  const handleMouseUp = useCallback(() => { dragRef.current = null; }, []);

  const handleClick = useCallback((e: React.MouseEvent) => {
    if (dragRef.current) return;
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const cx = canvasWidth / 2;
    const cy = canvasHeight / 2;
    for (const node of nodes) {
      const sx = cx + (node.x - view.offsetX) * view.scale;
      const sy = cy + (node.y - view.offsetY) * view.scale;
      const sr = node.radius * view.scale;
      const dx = mx - sx;
      const dy = my - sy;
      if (dx * dx + dy * dy < sr * sr) {
        openDetail({ kind: node.kind === "cluster" ? "plan" : node.kind, id: node.id });
        return;
      }
    }
  }, [nodes, view, canvasWidth, canvasHeight, openDetail]);

  return (
    <div ref={containerRef} className="relative flex-1 overflow-hidden">
      <canvas
        ref={canvasRef}
        className="absolute inset-0 h-full w-full cursor-grab active:cursor-grabbing"
        style={{ width: canvasWidth, height: canvasHeight }}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onClick={handleClick}
      />
      <AgentCursor />
    </div>
  );
}
