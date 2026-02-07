/* ── Graph Canvas Drawing ── */

import type { GraphNode, GraphEdge, LODLevel, ViewState } from "../store/graph-store";

const BG_COLOR = "#0a0e17";
const GRID_COLOR = "rgba(255,255,255,0.02)";
const EDGE_COLOR = "rgba(255,255,255,0.06)";
const EDGE_PARENT_COLOR = "rgba(255,255,255,0.12)";
const LABEL_COLOR = "#9ca3af";
const HOVER_RING_COLOR = "#60a5fa";

export function drawGraph(
  ctx: CanvasRenderingContext2D,
  nodes: GraphNode[],
  edges: GraphEdge[],
  view: ViewState,
  lod: LODLevel,
  hoverId: string | null,
  width: number,
  height: number,
): void {
  // Clear
  ctx.fillStyle = BG_COLOR;
  ctx.fillRect(0, 0, width, height);

  const cx = width / 2;
  const cy = height / 2;

  // Grid
  drawGrid(ctx, view, cx, cy, width, height);

  // Edges
  ctx.lineWidth = 1;
  for (const edge of edges) {
    const src = nodes.find((n) => n.id === edge.source);
    const tgt = nodes.find((n) => n.id === edge.target);
    if (!src || !tgt) continue;

    // Skip task edges at overview LOD
    if (lod === "overview" && (src.kind === "task" || tgt.kind === "task")) continue;

    const sx = cx + (src.x - view.offsetX) * view.scale;
    const sy = cy + (src.y - view.offsetY) * view.scale;
    const tx = cx + (tgt.x - view.offsetX) * view.scale;
    const ty = cy + (tgt.y - view.offsetY) * view.scale;

    ctx.strokeStyle = edge.kind === "parent" ? EDGE_PARENT_COLOR : EDGE_COLOR;
    ctx.globalAlpha = edge.kind === "similar" ? 0.3 : 0.6;
    ctx.beginPath();
    ctx.moveTo(sx, sy);
    ctx.lineTo(tx, ty);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;

  // Nodes
  for (const node of nodes) {
    // Skip tasks at overview LOD
    if (lod === "overview" && node.kind === "task") continue;

    const sx = cx + (node.x - view.offsetX) * view.scale;
    const sy = cy + (node.y - view.offsetY) * view.scale;
    const sr = node.radius * view.scale;

    // Cull off-screen
    if (sx + sr < 0 || sx - sr > width || sy + sr < 0 || sy - sr > height) continue;

    const isHover = node.id === hoverId;

    // Glow
    if (isHover || node.kind === "plan") {
      ctx.shadowColor = node.color;
      ctx.shadowBlur = isHover ? 20 : node.kind === "plan" ? 8 : 0;
    }

    // Body
    ctx.fillStyle = node.color;
    ctx.globalAlpha = node.status === "DONE" || node.status === "COMPLETED" ? 0.5 : 0.85;
    ctx.beginPath();
    ctx.arc(sx, sy, sr, 0, Math.PI * 2);
    ctx.fill();

    ctx.shadowBlur = 0;
    ctx.globalAlpha = 1;

    // Hover ring
    if (isHover) {
      ctx.strokeStyle = HOVER_RING_COLOR;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(sx, sy, sr + 4, 0, Math.PI * 2);
      ctx.stroke();
    }

    // Blocked indicator
    if (node.blocked) {
      ctx.strokeStyle = "#f87171";
      ctx.lineWidth = 1.5;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.arc(sx, sy, sr + 2, 0, Math.PI * 2);
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // Labels
    const showLabel =
      node.kind === "plan" ||
      (node.kind === "task" && view.scale >= 1.75) ||
      isHover;

    if (showLabel && sr > 3) {
      ctx.fillStyle = LABEL_COLOR;
      ctx.font = `${Math.max(9, Math.min(12, sr * 0.8))}px system-ui, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      const label = node.label.length > 30 ? node.label.slice(0, 28) + "..." : node.label;
      ctx.fillText(label, sx, sy + sr + 4);
    }

    // Task count badge for plans
    if (node.kind === "plan" && node.taskCount && node.taskCount > 0 && sr > 10) {
      ctx.fillStyle = "rgba(0,0,0,0.6)";
      ctx.font = `bold ${Math.max(8, sr * 0.5)}px system-ui`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(node.taskCount), sx, sy);
    }
  }
}

function drawGrid(
  ctx: CanvasRenderingContext2D,
  view: ViewState,
  cx: number,
  cy: number,
  width: number,
  height: number,
): void {
  const gridSize = 100;
  const scaledGrid = gridSize * view.scale;
  if (scaledGrid < 20) return; // too zoomed out

  ctx.strokeStyle = GRID_COLOR;
  ctx.lineWidth = 1;

  const startX = cx - (view.offsetX * view.scale) % scaledGrid;
  const startY = cy - (view.offsetY * view.scale) % scaledGrid;

  for (let x = startX; x < width; x += scaledGrid) {
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  for (let x = startX; x > 0; x -= scaledGrid) {
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  for (let y = startY; y < height; y += scaledGrid) {
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }
  for (let y = startY; y > 0; y -= scaledGrid) {
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }
}
