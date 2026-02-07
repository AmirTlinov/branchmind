import React, { useMemo } from "react";
import { GlassPanel } from "@/components/ui/Glass";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { formatTime } from "@/lib/format";
import { Copy, FileText, GitBranch, Hash, Tag } from "lucide-react";

function CopyButton({ value }: { value: string }) {
  return (
    <button
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(value);
        } catch {
          // ignore
        }
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

export function Inspector() {
  const selected_task = useStore((s) => s.selected_task);
  const selected_plan = useStore((s) => s.selected_plan);
  const reasoning_ref = useStore((s) => s.reasoning_ref);
  const selected_step = useStore((s) => s.selected_step);

  const graph_slice = useStore((s) => s.graph_slice);
  const graph_selected_id = useStore((s) => s.graph_selected_id);

  const node = useMemo(() => {
    if (!graph_slice || !graph_selected_id) return null;
    return graph_slice.nodes.find((n) => n.id === graph_selected_id) || null;
  }, [graph_selected_id, graph_slice]);

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
        {/* Task */}
        <div className="space-y-2">
          <Label>Task</Label>
          {!selected_task && <div className="text-[12px] text-gray-500">No task selected.</div>}
          {selected_task && (
            <div className="space-y-2">
              <div className="text-[13px] font-semibold text-gray-900">{selected_task.title}</div>
              <div className="grid grid-cols-1 gap-1">
                <KV k="status" v={selected_task.status} />
                <KV k="priority" v={selected_task.priority} />
                <KV k="blocked" v={selected_task.blocked ? "yes" : "no"} />
                <KV k="reasoning" v={selected_task.reasoning_mode} />
                <KV k="updated" v={formatTime(selected_task.updated_at_ms)} />
              </div>
            </div>
          )}
        </div>

        {/* Plan */}
        {selected_plan && (
          <div className="space-y-2">
            <Label>Plan</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="text-[12px] font-semibold text-gray-900 truncate">
                {selected_plan.title}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{selected_plan.id}</span>
                <CopyButton value={selected_plan.id} />
              </div>
            </div>
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

        {/* Graph node */}
        {node && (
          <div className="space-y-2">
            <Label>Graph node</Label>
            <div className="flex items-center justify-between gap-2">
              <div className="text-[12px] font-semibold text-gray-900 truncate">
                {node.title || node.id}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <span className="text-[10px] text-gray-400 font-mono">{node.id}</span>
                <CopyButton value={node.id} />
              </div>
            </div>
            <div className="space-y-1">
              <KV k="type" v={node.node_type} />
              {node.status && <KV k="status" v={node.status} />}
              {node.tags.length > 0 && (
                <KV
                  k="tags"
                  v={
                    <span className="inline-flex items-center gap-1 flex-wrap justify-end">
                      <Tag size={12} className="text-gray-400" />
                      {node.tags.slice(0, 8).map((t) => (
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
              {node.text && (
                <div className="mt-2">
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">text</div>
                  <pre className="text-[11px] text-gray-700 whitespace-pre-wrap leading-relaxed bg-white/55 ring-1 ring-black/[0.03] rounded-xl p-3">
                    {node.text}
                  </pre>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </GlassPanel>
  );
}

