/**
 * Manipulation bar for the library table's multi-level sort stack.
 *
 * RDG's own header interaction (ctrl/cmd-click) is the fast path for
 * building a stack, but a stray plain click on any header silently
 * collapses the whole stack to that one column with no undo. This bar is
 * the recovery surface and the only way to reorder levels or flip a
 * level's direction without modifier-key knowledge, so every control here
 * duplicates something a header gesture can already do.
 *
 * The sr-only summary paragraph renders unconditionally, even for an
 * empty stack, so a caller can wire `aria-describedby` to one stable id
 * regardless of how the sort stack changes shape.
 */
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, X } from "lucide-react";
import type { ReactElement } from "react";

import type { SortField, SortLevelParam } from "@/api";
import { Button } from "@/components/ui/button";

/** Stable id for the sr-only summary; callers wire `aria-describedby` to it. */
export const SORT_SUMMARY_ID = "library-sort-summary";

type Props = {
  levels: readonly SortLevelParam[];
  labels: Record<SortField, string>;
  onChange: (levels: readonly SortLevelParam[]) => void;
};

function summaryText(levels: readonly SortLevelParam[], labels: Record<SortField, string>): string {
  if (levels.length === 0) return "Not sorted";
  const parts = levels.map(
    (level) => `${labels[level.field]} ${level.desc ? "descending" : "ascending"}`,
  );
  return `Sorted by ${parts.join(", then ")}`;
}

export function SortChips({ levels, labels, onChange }: Props): ReactElement {
  const summary = summaryText(levels, labels);

  if (levels.length === 0) {
    return (
      <p id={SORT_SUMMARY_ID} className="sr-only" aria-live="polite">
        {summary}
      </p>
    );
  }

  function toggleDirection(index: number): void {
    onChange(levels.map((level, i) => (i === index ? { ...level, desc: !level.desc } : level)));
  }

  function remove(index: number): void {
    onChange(levels.filter((_, i) => i !== index));
  }

  function moveUp(index: number): void {
    if (index === 0) return;
    const next = [...levels];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    onChange(next);
  }

  function moveDown(index: number): void {
    if (index === levels.length - 1) return;
    const next = [...levels];
    [next[index + 1], next[index]] = [next[index], next[index + 1]];
    onChange(next);
  }

  return (
    <div className="mb-2 flex flex-wrap items-center gap-2">
      <p id={SORT_SUMMARY_ID} className="sr-only" aria-live="polite">
        {summary}
      </p>
      <div role="group" aria-label="Sort order" className="flex flex-wrap items-center gap-2">
        {levels.map((level, index) => {
          const label = labels[level.field];
          return (
            <div
              key={level.field}
              className="border-border bg-surface-1 inline-flex items-center gap-0.5 rounded-full border py-1 pl-3 pr-1 text-xs"
            >
              <span className="text-fg-muted font-mono">{index + 1}</span>
              <span className="text-fg font-medium">{label}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Change ${label} sort direction (currently ${level.desc ? "descending" : "ascending"})`}
                onClick={() => {
                  toggleDirection(index);
                }}
              >
                {level.desc ? <ArrowDown aria-hidden="true" /> : <ArrowUp aria-hidden="true" />}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Move ${label} earlier in sort priority`}
                disabled={index === 0}
                onClick={() => {
                  moveUp(index);
                }}
              >
                <ChevronUp aria-hidden="true" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Move ${label} later in sort priority`}
                disabled={index === levels.length - 1}
                onClick={() => {
                  moveDown(index);
                }}
              >
                <ChevronDown aria-hidden="true" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Remove ${label} from sort`}
                onClick={() => {
                  remove(index);
                }}
              >
                <X aria-hidden="true" />
              </Button>
            </div>
          );
        })}
      </div>
      <Button
        type="button"
        variant="outline"
        size="xs"
        onClick={() => {
          onChange([]);
        }}
      >
        Clear sort
      </Button>
    </div>
  );
}
