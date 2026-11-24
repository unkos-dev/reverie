import { Check, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactElement } from "react";

type Props = {
  saved: boolean;
  pending: boolean;
};

export function CellSaveState({ saved, pending }: Readonly<Props>): ReactElement | null {
  const [previousSaved, setPreviousSaved] = useState(saved);
  const [showSaved, setShowSaved] = useState(false);
  if (saved !== previousSaved) {
    setPreviousSaved(saved);
    setShowSaved(saved && !pending);
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
