import { cn } from "@/lib/cn";

interface SkeletonProps {
  variant?: "text" | "card" | "list-item";
  count?: number;
  className?: string;
}

function SkeletonLine({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "h-3 rounded-md bg-gradient-to-r from-gray-200/60 via-gray-100/40 to-gray-200/60",
        "bg-[length:200%_100%] animate-[shimmer_1.5s_ease-in-out_infinite]",
        className,
      )}
    />
  );
}

function SkeletonCard() {
  return (
    <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl p-4 space-y-3">
      <div className="flex items-center justify-between gap-4">
        <SkeletonLine className="w-2/3 h-4" />
        <SkeletonLine className="w-16 h-5 rounded-lg" />
      </div>
      <SkeletonLine className="w-full" />
      <SkeletonLine className="w-4/5" />
      <SkeletonLine className="w-1/2" />
    </div>
  );
}

function SkeletonListItem() {
  return (
    <div className="flex items-center gap-3 py-2 px-2">
      <SkeletonLine className="w-2 h-2 rounded-full shrink-0" />
      <div className="flex-1 space-y-1.5">
        <SkeletonLine className="w-3/4 h-3.5" />
        <SkeletonLine className="w-1/3 h-2.5" />
      </div>
    </div>
  );
}

export function Skeleton({ variant = "text", count = 3, className }: SkeletonProps) {
  const items = Array.from({ length: count });

  if (variant === "card") {
    return (
      <div className={cn("space-y-3", className)}>
        {items.map((_, i) => (
          <SkeletonCard key={i} />
        ))}
      </div>
    );
  }

  if (variant === "list-item") {
    return (
      <div className={cn("space-y-1", className)}>
        {items.map((_, i) => (
          <SkeletonListItem key={i} />
        ))}
      </div>
    );
  }

  return (
    <div className={cn("space-y-2 py-2", className)}>
      {items.map((_, i) => (
        <SkeletonLine key={i} className={i === count - 1 ? "w-2/3" : "w-full"} />
      ))}
    </div>
  );
}
