import { Check, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import type { UseCellEditResult } from "./table/useCellEdit";

type Props = {
  pending: boolean;
  feedback: UseCellEditResult["feedback"][number] | undefined;
};

export function CellSaveState({ pending, feedback }: Readonly<Props>): ReactElement | null {
  const [wasPending, setWasPending] = useState(pending);
  const [saved, setSaved] = useState(false);
  if (pending !== wasPending) {
    setWasPending(pending);
    setSaved(!pending && feedback === undefined);
  }
  useEffect(() => {
    if (!saved) return;
    const timer = window.setTimeout(() => {
      setSaved(false);
    }, 1800);
    return () => {
      window.clearTimeout(timer);
    };
  }, [saved]);
  if (feedback !== undefined)
    return (
      <div
        className="bg-canvas text-danger-text absolute right-1 flex items-center gap-1 rounded px-1 text-xs"
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
        className="bg-canvas text-fg-muted pointer-events-none absolute right-2 flex items-center gap-1 text-xs"
        role="status"
      >
        <Loader2 className="size-3 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        Saving
      </span>
    );
  return saved ? (
    <span
      className="rv-saved bg-canvas text-fg-muted pointer-events-none absolute right-2 flex items-center gap-1 text-xs"
      role="status"
    >
      <Check className="size-3" aria-hidden="true" />
      Saved
    </span>
  ) : null;
}
