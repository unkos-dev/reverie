/**
 * Native `<select>` editor for the per-user reading-status column. A native
 * element, not the Radix-based `Select` primitive, so the editor never
 * grows a portal: the grid binding's Tab-to-next-cell check requires the
 * `.rdg-editor-container` to hold exactly one input/select/textarea.
 * Selecting an option commits immediately (select-and-done); there is no
 * draft to stage, so Enter/Escape at the container have nothing further to
 * do once a choice is made.
 */
import { useEffect, useRef, type ChangeEvent, type ReactElement } from "react";

import type { ReadingStatus } from "@/api/books";
import { cn } from "@/lib/utils";

type StatusCellEditorProps = {
  value: ReadingStatus | null;
  onCommit: (value: ReadingStatus | null) => void;
};

const STATUS_OPTIONS: readonly { value: ReadingStatus; label: string }[] = [
  { value: "want_to_read", label: "Want to read" },
  { value: "reading", label: "Reading" },
  { value: "on_hold", label: "On hold" },
  { value: "finished", label: "Finished" },
  { value: "abandoned", label: "Abandoned" },
];

function isReadingStatus(candidate: string): candidate is ReadingStatus {
  return STATUS_OPTIONS.some((option) => option.value === candidate);
}

export function StatusCellEditor({ value, onCommit }: StatusCellEditorProps): ReactElement {
  const selectRef = useRef<HTMLSelectElement>(null);

  useEffect(() => {
    selectRef.current?.focus();
  }, []);

  function handleChange(event: ChangeEvent<HTMLSelectElement>): void {
    const raw = event.target.value;
    if (raw === "") {
      onCommit(null);
      return;
    }
    if (isReadingStatus(raw)) onCommit(raw);
  }

  return (
    <select
      ref={selectRef}
      aria-label="Reading status"
      defaultValue={value ?? ""}
      onChange={handleChange}
      className={cn(
        "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none",
        "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50",
      )}
    >
      <option value="">-</option>
      {STATUS_OPTIONS.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}
