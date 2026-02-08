import React, { useMemo } from "react";
import { GlassPanel } from "@/components/ui/Glass";
import { Markdown } from "@/components/ui/Markdown";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { EmptyState } from "@/components/ui/EmptyState";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { formatTime } from "@/lib/format";
import {
  AlertTriangle,
  CheckCircle2,
  CircleDashed,
  Copy,
  Eye,
  FileText,
  GitBranch,
  Hash,
  Layers,
  Tag,
} from "lucide-react";

function CopyButton({ value }: { value: string }) {
  return (
    <button
      onClick={async () => {
        try { await navigator.clipboard.writeText(value); } catch { /* ignore */ }
      }}
      className="p-1 rounded-md hover:bg-black/5 text-gray-400 hover:text-gray-700 transition-colors"
      title="Copy"
    >
      <Copy size={12} />
    </button>
  );
}

function Label({ children }: React.PropsWithChildren) {
  return <div className="text-[10px] text-gray-400 uppercase tracking-widest">{children}</div>;
}

function KV({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-2">
      <div className="text-[11px] text-gray-500">{k}</div>
      <div className="text-[11px] text-gray-800 text-right break-all">{v}</div>
    </div>
  );
}

function ConfirmationGates({ task }: { task: { criteria_confirmed: boolean; tests_confirmed: boolean; security_confirmed: boolean; perf_confirmed: boolean; docs_confirmed: boolean } }) {
  const gates = [
    { key: "criteria", label: "Criteria", ok: task.criteria_confirmed },
    { key: "tests", label: "Tests", ok: task.tests_confirmed },
    { key: "security", label: "Security", ok: task.security_confirmed },
    { key: "perf", label: "Perf", ok: task.perf_confirmed },
    { key: "docs", label: "Docs", ok: task.docs_confirmed },
  ];
  return (
    <div className="flex items-start gap-2.5 mt-2">
      {gates.map((g) => (
        <div key={g.key} className="flex flex-col items-center" style={{ width: 20 }}>
          {g.ok
            ? <CheckCircle2 size={16} className="text-emerald-500" />
            : <CircleDashed size={16} className="text-gray-300" />}
          <span className="text-[9px] text-gray-400 mt-0.5 leading-none">{g.label}</span>
        </div>
      ))}
    </div>
  );
}

export function Inspector() {
  const selected_task = useStore((s) => s.selected_task);
  const selected_plan = useStore((s) => s.selected_plan);
  const reasoning_ref = useStore((s) => s.reasoning_ref);
  const selected_step = useStore((s) => s.selected_step);
  const steps_summary = useStore((s) => s.steps_summary);

  const graph_slice = useStore((s) => s.graph_slice);
  const graph_mode = useStore((s) => s.graph_mode);
  const graph_selected_id = useStore((s) => s.graph_selected_id);
  const architecture_lens = useStore((s) => s.architecture_lens);
  const architecture_provenance = useStore((s) => s.architecture_provenance);
  const architecture_provenance_status = useStore((s) => s.architecture_provenance_status);
  const architecture_provenance_error = useStore((s) => s.architecture_provenance_error);

  const reasoningNode = useMemo(() => {
    if (!graph_slice || !graph_selected_id) return null;
    return graph_slice.nodes.find((n) => n.id === graph_selected_id) || null;
  }, [graph_selected_id, graph_slice]);

  const architectureNode = useMemo(() => {
    if (!architecture_lens || !graph_selected_id) return null;
    return architecture_lens.nodes.find((n) => n.id === graph_selected_id) || null;
  }, [architecture_lens, graph_selected_id]);

  const edgeCount = useMemo(() => {
    if (graph_mode === "architecture") {
      if (!architecture_lens || !graph_selected_id) return 0;
      return architecture_lens.edges.filter((e) => e.from === graph_selected_id || e.to === graph_selected_id).length;
    }
    if (!graph_slice || !graph_selected_id) return 0;
    return graph_slice.edges.filter((e) => e.from === graph_selected_id || e.to === graph_selected_id).length;
  }, [graph_mode, graph_slice, architecture_lens, graph_selected_id]);

  const planPct = useMemo(() => {
    if (!steps_summary) return null;
    const total = Math.max(steps_summary.total_steps, 1);
    return Math.round((steps_summary.completed_steps / total) * 100);
  }, [steps_summary]);

  return (
    <GlassPanel
      intensity="low"
      className="h-full flex flex-col border-l border-gray-200/50 bg-[#EBECF0]/80 backdrop-blur-xl rounded-none"
    >
      <div className="h-10 px-3 flex items-center justify-between border-b border-gray-200/50 shrink-0">
        <div className="text-[11px] font-bold text-gray-600 uppercase tracking-widest">Inspector</div>
        {selected_task && (
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-gray-400 font-mono">{selected_task.id}</span>
            <CopyButton value={selected_task.id} />
          </div>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar px-4 py-4 space-y-6">
        {/* Empty state */}
        {!selected_task && (
          <EmptyState icon={Eye} heading="Inspector" description="Select a task to see details" />
        )}

        {/* Task */}
        {selected_task && (
          <div className="space-y-2">
            <Label>Task</Label>
            <div className="space-y-2">
              <div className="text-[13px] font-semibold text-gray-900">{selected_task.title}</div>
              <div className="grid grid-cols-1 gap-1">
                <KV k="status" v={selected_task.status} />
                <KV k="priority" v={selected_task.priority} />
                <KV k="blocked" v={selected_task.blocked ? "yes" : "no"} />
                <KV k="reasoning" v={selected_task.reasoning_mode} />
                <KV k="updated" v={formatTime(selected_task.updated_at_ms)} />
              </div>
              <ConfirmationGates task={selected_task} />
              {selected_task.description && <Markdown text={selected_task.description} className="mt-3" />}
              {selected_task.context && <Markdown text={selected_task.context} className="mt-3" />}
            </div>
          </div>
        )}

        {/* Plan */}
        {selected_plan && (
          <div className="space-y-2">
            <Label>Plan</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 min-w-0">
                <div className="text-[12px] font-semibold text-gray-900 truncate">
                  {selected_plan.title}
                </div>
                <StatusBadge status={selected_plan.status} />
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{selected_plan.id}</span>
                <CopyButton value={selected_plan.id} />
              </div>
            </div>
            {steps_summary && planPct !== null && (
              <div className="space-y-1">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] text-gray-500">
                    {steps_summary.completed_steps}/{steps_summary.total_steps} steps
                  </span>
                  <span className="text-[10px] text-gray-400 font-mono">{planPct}%</span>
                </div>
                <div className="h-1 rounded-full bg-gray-200/60 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-emerald-500 transition-all duration-300"
                    style={{ width: `${planPct}%` }}
                  />
                </div>
              </div>
            )}
          </div>
        )}

        {/* Step */}
        {selected_step && (
          <div className="space-y-2">
            <Label>Step</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="text-[12px] font-semibold text-gray-900 truncate">
                {selected_step.title}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{selected_step.step_id}</span>
                <CopyButton value={selected_step.step_id} />
              </div>
            </div>
            <div className="space-y-1">
              <KV k="path" v={selected_step.path} />
              {selected_step.next_action && <KV k="next" v={selected_step.next_action} />}
              {selected_step.stop_criteria && <KV k="stop" v={selected_step.stop_criteria} />}
            </div>
            {selected_step.success_criteria.length > 0 && (
              <div className="mt-1">
                <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">Success criteria</div>
                <ul className="space-y-1 text-[11px] text-gray-700">
                  {selected_step.success_criteria.map((c, i) => <li key={i}>• {c}</li>)}
                </ul>
              </div>
            )}
            {selected_step.tests.length > 0 && (
              <div className="mt-1">
                <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">Tests</div>
                <ul className="space-y-1 text-[11px] text-gray-700">
                  {selected_step.tests.map((t, i) => <li key={i}>• {t}</li>)}
                </ul>
              </div>
            )}
            {selected_step.blockers.length > 0 && (
              <div className="bg-rose-50 ring-1 ring-rose-200/50 rounded-xl p-3 mt-2">
                <div className="text-[10px] text-rose-600 font-medium uppercase tracking-widest mb-1">Blockers</div>
                <ul className="space-y-1 text-[11px] text-rose-700">
                  {selected_step.blockers.map((b, i) => <li key={i}>• {b}</li>)}
                </ul>
              </div>
            )}
          </div>
        )}

        {/* Reasoning */}
        {reasoning_ref && (
          <div className="space-y-2">
            <Label>Reasoning</Label>
            <div className="space-y-1">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 text-[11px] text-gray-700 min-w-0">
                  <GitBranch size={13} className="text-gray-400 shrink-0" />
                  <span className="font-mono truncate">{reasoning_ref.branch}</span>
                </div>
                <CopyButton value={reasoning_ref.branch} />
              </div>
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 text-[11px] text-gray-700 min-w-0">
                  <FileText size={13} className="text-gray-400 shrink-0" />
                  <span className="font-mono truncate">{reasoning_ref.notes_doc}</span>
                </div>
                <CopyButton value={reasoning_ref.notes_doc} />
              </div>
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 text-[11px] text-gray-700 min-w-0">
                  <Hash size={13} className="text-gray-400 shrink-0" />
                  <span className="font-mono truncate">{reasoning_ref.graph_doc}</span>
                </div>
                <CopyButton value={reasoning_ref.graph_doc} />
              </div>
            </div>
          </div>
        )}

        {/* Architecture summary */}
        {graph_mode === "architecture" && architecture_lens && (
          <div className="space-y-2">
            <Label>Architecture lens</Label>
            <div className="space-y-1">
              <KV k="scope" v={architecture_lens.scope.id ? `${architecture_lens.scope.kind}:${architecture_lens.scope.id}` : architecture_lens.scope.kind} />
              <KV k="mode" v={architecture_lens.mode} />
              <KV k="generated" v={formatTime(architecture_lens.generated_at_ms)} />
              <KV
                k="summary"
                v={`${architecture_lens.summary.anchors_total}a / ${architecture_lens.summary.tasks_total}t / ${architecture_lens.summary.knowledge_total}k`}
              />
              <KV k="proven" v={`${Math.round(architecture_lens.summary.proven_ratio * 100)}%`} />
              {architecture_lens.risks.length > 0 && (
                <div className="mt-2 bg-rose-50 ring-1 ring-rose-200/50 rounded-xl p-3">
                  <div className="text-[10px] text-rose-600 font-medium uppercase tracking-widest mb-1 inline-flex items-center gap-1">
                    <AlertTriangle size={11} />
                    Risks ({architecture_lens.risks.length})
                  </div>
                  <ul className="space-y-1 text-[11px] text-rose-700">
                    {architecture_lens.risks.slice(0, 4).map((r) => (
                      <li key={r.id}>• {r.title}</li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Architecture node */}
        {graph_mode === "architecture" && architectureNode && (
          <div className="space-y-2">
            <Label>Architecture node</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="text-[12px] font-semibold text-gray-900 truncate">
                {architectureNode.label}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{architectureNode.id}</span>
                <CopyButton value={architectureNode.id} />
              </div>
            </div>
            <div className="space-y-1">
              <KV k="type" v={architectureNode.node_type} />
              <KV
                k="layer"
                v={
                  <span className="inline-flex items-center gap-1">
                    <Layers size={12} className="text-gray-400" />
                    {architectureNode.layer}
                  </span>
                }
              />
              {architectureNode.status && <KV k="status" v={architectureNode.status} />}
              <KV k="edges" v={edgeCount} />
              <KV k="risk" v={architectureNode.risk_score.toFixed(2)} />
              <KV k="evidence" v={architectureNode.evidence_score.toFixed(2)} />
              <KV k="updated" v={formatTime(architectureNode.last_ts_ms)} />
              {architectureNode.tags.length > 0 && (
                <KV
                  k="tags"
                  v={
                    <span className="inline-flex items-center gap-1 flex-wrap justify-end">
                      <Tag size={12} className="text-gray-400" />
                      {architectureNode.tags.slice(0, 10).map((t) => (
                        <span
                          key={t}
                          className={cn(
                            "px-1.5 py-0.5 rounded-md bg-white/60 ring-1 ring-black/[0.03] text-[10px] text-gray-600 font-mono",
                          )}
                        >
                          {t}
                        </span>
                      ))}
                    </span>
                  }
                />
              )}
            </div>

            <div className="mt-2">
              <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">Provenance</div>
              {architecture_provenance_status === "loading" && (
                <div className="text-[11px] text-gray-500">Loading…</div>
              )}
              {architecture_provenance_status === "error" && (
                <div className="text-[11px] text-rose-600">Failed: {architecture_provenance_error}</div>
              )}
              {architecture_provenance_status === "ready" &&
                architecture_provenance &&
                architecture_provenance.node_id === architectureNode.id && (
                  <ul className="space-y-1 text-[11px] text-gray-700">
                    {architecture_provenance.records.slice(0, 8).map((r, idx) => (
                      <li key={`${r.kind}:${r.id}:${idx}`} className="rounded-lg bg-white/60 ring-1 ring-black/[0.03] px-2 py-1">
                        <div className="font-mono text-[10px] text-gray-500">{r.kind}</div>
                        <div className="text-[11px] text-gray-800">{r.label || r.id}</div>
                        {r.note && <div className="text-[10px] text-gray-500 mt-0.5">{r.note}</div>}
                      </li>
                    ))}
                  </ul>
                )}
            </div>
          </div>
        )}

        {/* Reasoning graph node */}
        {graph_mode !== "architecture" && reasoningNode && (
          <div className="space-y-2">
            <Label>Graph node</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="text-[12px] font-semibold text-gray-900 truncate">
                {reasoningNode.title || reasoningNode.id}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{reasoningNode.id}</span>
                <CopyButton value={reasoningNode.id} />
              </div>
            </div>
            <div className="space-y-1">
              <KV k="type" v={reasoningNode.node_type} />
              {reasoningNode.status && <KV k="status" v={reasoningNode.status} />}
              <KV k="edges" v={edgeCount} />
              {reasoningNode.tags.length > 0 && (
                <KV
                  k="tags"
                  v={
                    <span className="inline-flex items-center gap-1 flex-wrap justify-end">
                      <Tag size={12} className="text-gray-400" />
                      {reasoningNode.tags.slice(0, 8).map((t) => (
                        <span
                          key={t}
                          className={cn(
                            "px-1.5 py-0.5 rounded-md bg-white/60 ring-1 ring-black/[0.03] text-[10px] text-gray-600 font-mono",
                          )}
                        >
                          {t}
                        </span>
                      ))}
                    </span>
                  }
                />
              )}
              {reasoningNode.text && (
                <div className="mt-2">
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">text</div>
                  <Markdown text={reasoningNode.text} />
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </GlassPanel>
  );
}
