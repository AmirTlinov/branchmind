import { useEffect } from "react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { formatTime } from "@/lib/format";
import { BookOpen, RefreshCw } from "lucide-react";

function EntryCard({
  seq,
  ts_ms,
  kind,
  title,
  branch,
  content,
  event_type,
  task_id,
  path,
}: {
  seq: number;
  ts_ms: number;
  kind: string;
  title?: string | null;
  branch: string;
  content?: string | null;
  event_type?: string | null;
  task_id?: string | null;
  path?: string | null;
}) {
  const isNote = kind === "note";
  return (
    <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl p-4 space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[12px] font-semibold text-gray-900 truncate">
            {title || (isNote ? "Note" : "Event")}
          </div>
          <div className="mt-1 text-[10px] text-gray-400 font-mono flex flex-wrap gap-x-2 gap-y-1">
            <span>seq:{seq}</span>
            <span>•</span>
            <span>{formatTime(ts_ms)}</span>
            <span>•</span>
            <span className="truncate">{branch}</span>
            {event_type && (
              <>
                <span>•</span>
                <span>{event_type}</span>
              </>
            )}
            {task_id && (
              <>
                <span>•</span>
                <span>{task_id}</span>
              </>
            )}
            {path && (
              <>
                <span>•</span>
                <span>{path}</span>
              </>
            )}
          </div>
        </div>
        <span
          className={cn(
            "px-2 py-1 rounded-lg text-[10px] font-mono uppercase",
            isNote ? "bg-emerald-50 text-emerald-700" : "bg-gray-50 text-gray-500",
          )}
        >
          {kind}
        </span>
      </div>

      {content && (
        <pre className="text-[12px] text-gray-800 whitespace-pre-wrap leading-relaxed">
          {content}
        </pre>
      )}
      {!content && <div className="text-[12px] text-gray-500">—</div>}
    </div>
  );
}

export function DocsView() {
  const selected_task_id = useStore((s) => s.selected_task_id);
  const reasoning_ref = useStore((s) => s.reasoning_ref);
  const entries = useStore((s) => s.notes_entries);
  const load_tail = useStore((s) => s.load_docs_tail);

  useEffect(() => {
    if (!reasoning_ref) return;
    void load_tail("notes");
  }, [load_tail, reasoning_ref]);

  if (!selected_task_id) {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-gray-500">
        Select a task to view notes.
      </div>
    );
  }

  return (
    <div className="w-full h-full overflow-y-auto custom-scrollbar px-6 py-6 space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest flex items-center gap-2">
            <BookOpen size={14} /> Notes
          </div>
          <div className="text-[12px] text-gray-600 mt-1">
            Latest {entries.length} entries (tail)
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
        <div className="text-[12px] text-gray-500">No notes yet.</div>
      ) : (
        <div className="space-y-3">
          {entries.map((e) => (
            <EntryCard
              key={e.seq}
              seq={e.seq}
              ts_ms={e.ts_ms}
              kind={e.kind}
              title={e.title}
              branch={e.branch}
              content={e.content}
              event_type={e.event_type}
              task_id={e.task_id}
              path={e.path}
            />
          ))}
        </div>
      )}
    </div>
  );
}
