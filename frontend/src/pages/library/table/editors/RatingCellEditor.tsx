/**
 * Star-rating editor for the per-user rating column. Digit keys 1-5 set and
 * commit immediately; 0/Delete/Backspace clear and commit; the arrow keys
 * only stage a draft (Enter commits it), so a keyboard user can preview an
 * adjustment before it writes. The group is a single tab stop: every key
 * this widget cares about is a digit or arrow rather than per-star Tab
 * navigation, so the stars themselves sit outside the tab order while each
 * still carries its own `aria-label` for the accessibility tree.
 */
import { Star, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactElement } from "react";

import { cn } from "@/lib/utils";

type RatingCellEditorProps = {
  value: number | null;
  onCommit: (value: number | null) => void;
  onDraft?: (value: number | null) => void;
};

const STAR_VALUES = [1, 2, 3, 4, 5] as const;
const DIGIT_RATINGS: Readonly<Partial<Record<string, number>>> = {
  "1": 1,
  "2": 2,
  "3": 3,
  "4": 4,
  "5": 5,
};

export function RatingCellEditor({
  value,
  onCommit,
  onDraft,
}: RatingCellEditorProps): ReactElement {
  const [draft, setDraft] = useState(value);
  const groupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    groupRef.current?.focus();
  }, []);

  function setAndCommit(next: number | null): void {
    setDraft(next);
    onCommit(next);
  }

  function stage(next: number | null): void {
    setDraft(next);
    onDraft?.(next);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    const digitRating = DIGIT_RATINGS[event.key];
    if (digitRating !== undefined) {
      setAndCommit(digitRating);
      return;
    }
    if (event.key === "0" || event.key === "Delete" || event.key === "Backspace") {
      setAndCommit(null);
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      stage(Math.min(5, (draft ?? 0) + 1));
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      const next = (draft ?? 0) - 1;
      stage(next <= 0 ? null : next);
      return;
    }
    if (event.key === "Enter") {
      // The draft-then-Enter model is local to this editor (arrows never
      // call onCommit), so finish the interaction here rather than let the
      // grid binding's own Enter handling, which commits whatever row
      // `update()` last staged, race a second commit of the same value.
      event.stopPropagation();
      onCommit(draft);
    }
  }

  return (
    <div
      ref={groupRef}
      role="group"
      aria-label="Rating"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      className="focus-visible:ring-ring/50 flex h-8 items-center gap-1 px-1 outline-none focus-visible:ring-2"
    >
      {STAR_VALUES.map((star) => {
        const filled = draft !== null && star <= draft;
        return (
          <button
            key={star}
            type="button"
            tabIndex={-1}
            aria-label={`Rate ${String(star)} of 5`}
            aria-pressed={filled}
            onClick={() => {
              setAndCommit(star);
            }}
            className={cn(
              "flex size-6 items-center justify-center rounded-[min(var(--radius-md),10px)] outline-none",
              filled ? "text-accent-text" : "text-fg-muted",
            )}
          >
            <Star className="size-4" fill={filled ? "currentColor" : "none"} aria-hidden="true" />
          </button>
        );
      })}
      <button
        type="button"
        tabIndex={-1}
        aria-label="Clear rating"
        onClick={() => {
          setAndCommit(null);
        }}
        className="text-fg-muted flex size-6 items-center justify-center rounded-[min(var(--radius-md),10px)] outline-none"
      >
        <X className="size-4" aria-hidden="true" />
      </button>
    </div>
  );
}
