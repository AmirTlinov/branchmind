import { useEffect } from "react";
import { useStore } from "@/store";
import { Timeline } from "./Timeline";
import { EmptyState } from "@/components/ui/EmptyState";
import { BookOpen, RefreshCw } from "lucide-react";

export function DocsView() {
  const selected_task_id = useStore((s) => s.selected_task_id);
  const reasoning_ref = useStore((s) => s.reasoning_ref);
  const entries = useStore((s) => s.notes_entries);
  const load_tail = useStore((s) => s.load_docs_tail);
  const notes_last_seq = useStore((s) => s.notes_last_seq);

  useEffect(() => {
    if (!reasoning_ref) return;
    void load_tail("notes");
  }, [load_tail, reasoning_ref]);

  if (!selected_task_id) {
    return <EmptyState icon={BookOpen} heading="Select a task" description="Choose a task to view notes." />;
  }

  return (
    <div className="w-full h-full overflow-y-auto custom-scrollbar px-6 py-6 space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest flex items-center gap-2">
            <BookOpen size={14} /> Notes
          </div>
          <div className="text-[12px] text-gray-600 mt-1 flex items-center gap-2">
            {entries.length} entries
            {notes_last_seq > 0 && (
              <span className="inline-flex items-center gap-1">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                <span className="text-[10px] text-emerald-600">live</span>
              </span>
            )}
          </div>
        </div>
        <button
          onClick={() => void load_tail("notes")}
          className="p-2 rounded-xl bg-white/60 ring-1 ring-black/[0.03] hover:bg-white/75 transition-colors text-gray-600"
          title="Refresh"
        >
          <RefreshCw size={14} />
        </button>
      </div>

      {entries.length === 0 ? (
        <EmptyState icon={BookOpen} heading="No notes yet" description="Notes will appear here as the agent works." />
      ) : (
        <Timeline entries={entries} />
      )}
    </div>
  );
}
