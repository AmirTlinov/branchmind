import { useStore } from "@/store";
import { EmptyState } from "@/components/ui/EmptyState";
import { Skeleton } from "@/components/ui/Skeleton";
import { PlanDetail } from "./PlanDetail";
import { ListChecks } from "lucide-react";

export function PlanView() {
  const selected_task_id = useStore((s) => s.selected_task_id);
  const steps_status = useStore((s) => s.steps_status);

  if (!selected_task_id) {
    return (
      <EmptyState
        icon={ListChecks}
        heading="Select a task"
        description="Choose a task from the sidebar to view its plan."
      />
    );
  }

  if (steps_status === "loading") {
    return (
      <div className="px-6 py-6">
        <Skeleton variant="card" count={3} />
      </div>
    );
  }

  return <PlanDetail />;
}
