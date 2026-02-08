import { useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { cn } from "@/lib/cn";
import { formatTime } from "@/lib/format";
import { Markdown } from "@/components/ui/Markdown";
import { ChevronRight } from "lucide-react";
import type { DocEntryDto } from "@/api/types";

interface TimelineProps {
  entries: DocEntryDto[];
  showPayload?: boolean;
  className?: string;
}

function dotColor(kind: string): string {
  if (kind === "note") return "bg-emerald-400";
  if (kind === "event") return "bg-gray-300";
  return "bg-blue-400";
}

function kindBadgeClass(kind: string): string {
  if (kind === "note") return "bg-emerald-50 text-emerald-700 ring-emerald-200/50";
  if (kind === "event") return "bg-gray-50 text-gray-600 ring-gray-200/50";
  return "bg-blue-50 text-blue-700 ring-blue-200/50";
}

function TimelineEntry({
  entry,
  index,
  showPayload,
}: {
  entry: DocEntryDto;
  index: number;
  showPayload?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const hasContent = !!(entry.content || (showPayload && entry.payload_json));

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, delay: index * 0.02 }}
      className="relative flex gap-4 pl-1"
    >
      {/* Dot */}
      <div className="relative flex flex-col items-center shrink-0 w-4">
        <div className={cn("w-2.5 h-2.5 rounded-full mt-1.5 shrink-0 ring-2 ring-white", dotColor(entry.kind))} />
      </div>

      {/* Card */}
      <div className="flex-1 min-w-0 pb-5">
        <button
          onClick={() => hasContent && setOpen((o) => !o)}
          className={cn(
            "w-full text-left bg-white/40 ring-1 ring-black/[0.03] rounded-2xl px-4 py-3 transition-colors",
            hasContent && "cursor-pointer hover:bg-white/60",
            !hasContent && "cursor-default",
          )}
        >
          {/* Header */}
          <div className="flex items-center gap-2 min-w-0">
            {hasContent && (
              <motion.div
                animate={{ rotate: open ? 90 : 0 }}
                transition={{ duration: 0.15 }}
                className="shrink-0 text-gray-400"
              >
                <ChevronRight size={12} />
              </motion.div>
            )}

            <span
              className={cn(
                "inline-flex items-center px-1.5 py-0.5 rounded-md text-[9px] font-mono uppercase ring-1 shrink-0",
                kindBadgeClass(entry.kind),
              )}
            >
              {entry.kind}
            </span>

            <span className="text-[12px] font-semibold text-gray-900 truncate">
              {entry.title || entry.event_type || (entry.kind === "note" ? "Note" : "Event")}
            </span>

            <span className="ml-auto text-[10px] text-gray-400 font-mono shrink-0">
              {formatTime(entry.ts_ms)}
            </span>
          </div>

          {/* Metadata */}
          <div className="mt-1.5 text-[10px] text-gray-400 font-mono flex flex-wrap gap-x-2 gap-y-0.5 ml-0">
            <span>seq:{entry.seq}</span>
            <span className="truncate max-w-[120px]">{entry.branch}</span>
            {entry.task_id && <span className="truncate max-w-[120px]">{entry.task_id}</span>}
            {entry.path && <span className="truncate max-w-[160px]">{entry.path}</span>}
          </div>
        </button>

        {/* Collapsible content */}
        <AnimatePresence initial={false}>
          {open && hasContent && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.2, ease: [0.25, 0.1, 0.25, 1] }}
              className="overflow-hidden"
            >
              <div className="mt-2 bg-white/40 ring-1 ring-black/[0.03] rounded-xl px-4 py-3">
                {entry.content ? (
                  <Markdown text={entry.content} className="text-[12px] text-gray-800 leading-relaxed" />
                ) : showPayload && entry.payload_json ? (
                  <pre className="text-[11px] text-gray-700 whitespace-pre-wrap leading-relaxed bg-white/55 ring-1 ring-black/[0.03] rounded-xl p-3 font-mono overflow-x-auto">
                    {entry.payload_json}
                  </pre>
                ) : null}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}

export function Timeline({ entries, showPayload, className }: TimelineProps) {
  return (
    <div className={cn("relative", className)}>
      {/* Vertical line */}
      <div className="absolute left-[8.5px] top-3 bottom-0 w-[2px] bg-gray-200/60" />

      {/* Entries */}
      <div className="relative">
        {entries.map((entry, i) => (
          <TimelineEntry key={entry.seq} entry={entry} index={i} showPayload={showPayload} />
        ))}
      </div>
    </div>
  );
}
