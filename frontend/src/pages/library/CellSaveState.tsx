import { Check, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import type { UseCellEditResult } from "./table/useCellEdit";

type Props = {
  saved: boolean;
  pending: boolean;
  feedback: UseCellEditResult["feedback"][number] | undefined;
};

function cellFailure(feedback: NonNullable<Props["feedback"]>, pending: boolean): ReactElement {
  return (
    <div
      className="bg-canvas text-danger-text absolute right-3 flex items-center gap-1 rounded px-1 text-xs"
      data-slot={pending || feedback.reloading ? "cell-save-state" : "cell-save-feedback"}
      role="status"
    >
      Not saved.
      {feedback.action === "retry" ? (
        <Button size="xs" variant="ghost" isPending={feedback.retrying} onPress={feedback.retry}>
          {feedback.retrying ? "Retrying…" : "Retry"}
        </Button>
      ) : null}
      {feedback.action === "reload" ? (
        <Button size="xs" variant="ghost" isPending={feedback.reloading} onPress={feedback.reload}>
          {feedback.reloading ? "Reloading…" : "Reload"}
        </Button>
      ) : null}
    </div>
  );
}

export function CellSaveState({ saved, pending, feedback }: Readonly<Props>): ReactElement | null {
  const [previousSaved, setPreviousSaved] = useState(saved);
  const [showSaved, setShowSaved] = useState(false);
  if (saved !== previousSaved) {
    setPreviousSaved(saved);
    setShowSaved(saved && !pending && feedback === undefined);
  }
  useEffect(() => {
    if (!showSaved) return;
    const timer = window.setTimeout(() => {
      setShowSaved(false);
    }, 1800);
    return () => {
      window.clearTimeout(timer);
    };
  }, [showSaved]);
  if (feedback !== undefined) return cellFailure(feedback, pending);
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
  return showSaved ? (
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
