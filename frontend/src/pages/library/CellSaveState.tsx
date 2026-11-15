import { Check, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import type { UseCellEditResult } from "./table/useCellEdit";

type Props = {
  attempt: number;
  pending: boolean;
  feedback: UseCellEditResult["feedback"][number] | undefined;
};

export function CellSaveState({
  attempt,
  pending,
  feedback,
}: Readonly<Props>): ReactElement | null {
  const [completedAttempt, setCompletedAttempt] = useState(pending ? 0 : attempt);
  const [savedAttempt, setSavedAttempt] = useState(0);
  if (!pending && attempt !== completedAttempt) {
    setCompletedAttempt(attempt);
    setSavedAttempt(feedback === undefined ? attempt : 0);
  }
  useEffect(() => {
    if (savedAttempt === 0) return;
    const timer = window.setTimeout(() => {
      setSavedAttempt(0);
    }, 1800);
    return () => {
      window.clearTimeout(timer);
    };
  }, [savedAttempt]);
  if (feedback !== undefined)
    return (
      <div
        className="bg-canvas text-danger-text absolute right-3 flex items-center gap-1 rounded px-1 text-xs"
        data-slot="cell-save-state"
        role="status"
      >
        Not saved.
        <Button size="xs" variant="ghost" isPending={feedback.reloading} onPress={feedback.reload}>
          {feedback.reloading ? "Reloading…" : "Reload"}
        </Button>
      </div>
    );
  if (pending)
    return (
      <span
        className="bg-canvas text-fg-muted pointer-events-none absolute right-3 flex items-center gap-1 text-xs"
        data-slot="cell-save-state"
        role="status"
      >
        <Loader2 className="size-3 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        Saving
      </span>
    );
  return savedAttempt !== 0 ? (
    <span
      className="rv-saved bg-canvas text-fg-muted pointer-events-none absolute right-3 flex items-center gap-1 text-xs"
      data-slot="cell-save-state"
      role="status"
    >
      <Check className="size-3" aria-hidden="true" />
      Saved
    </span>
  ) : null;
}
