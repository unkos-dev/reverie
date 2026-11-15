import type { ReactElement } from "react";
import type { Shelf } from "@/api";
import { cn } from "@/lib/utils";

type ReadingCounts = {
  wantToRead: number;
  reading: number;
  onHold: number;
  finished: number;
  abandoned: number;
  none: number;
};
type AttentionCounts = {
  toReview: number;
  enrichmentFailed: number;
  noIsbn: number;
  noCover: number;
};

export function ReadingStateBar({ data }: Readonly<{ data?: ReadingCounts }>): ReactElement | null {
  if (data === undefined) return null;
  const entries = [
    { label: "Want to read", count: data.wantToRead },
    { label: "Reading", count: data.reading },
    { label: "On hold", count: data.onHold },
    { label: "Finished", count: data.finished },
    { label: "Abandoned", count: data.abandoned },
    { label: "None", count: data.none },
  ];
  const total = entries.reduce((sum, entry) => sum + entry.count, 0);
  return (
    <div className="flex flex-wrap items-center gap-3 text-xs text-fg-muted">
      <div
        className="bg-surface-3 flex h-1.5 w-72 gap-0.5 overflow-hidden rounded-full"
        aria-hidden="true"
      >
        {entries.map((entry) => (
          <span
            key={entry.label}
            className={entry.label === "Reading" ? "bg-accent" : "bg-fg-muted"}
            style={{ width: `${String(total === 0 ? 0 : (entry.count / total) * 100)}%` }}
          />
        ))}
      </div>
      {entries.map((entry) => (
        <span key={entry.label}>
          {entry.count} {entry.label}
        </span>
      ))}
    </div>
  );
}

export function NeedsAttentionCounts({
  data,
}: Readonly<{ data?: AttentionCounts }>): ReactElement | null {
  if (data === undefined) return null;
  return (
    <div className="flex flex-wrap items-center gap-3 text-xs text-fg-muted">
      <span>Needs attention</span>
      <span>{data.toReview} to review</span>
      <span>{data.enrichmentFailed} enrichment failed</span>
      <span>{data.noIsbn} no ISBN</span>
      <span>{data.noCover} no cover</span>
    </div>
  );
}

type Props = {
  shelves: readonly Shelf[];
  selected: readonly string[];
  collapsed: boolean;
  readingCounts?: ReadingCounts;
  attentionCounts?: AttentionCounts;
};

export function ShelfHeader({
  shelves,
  selected,
  collapsed,
  readingCounts,
  attentionCounts,
}: Readonly<Props>): ReactElement {
  const shelf = selected.length === 1 ? shelves.find((item) => item.id === selected[0]) : undefined;
  const name =
    selected.length > 1 ? `${String(selected.length)} shelves` : (shelf?.name ?? "Library");
  return (
    <header className={cn("flex flex-col gap-4 px-2 py-5", collapsed && "py-2")}>
      <div className="flex min-w-0 items-baseline gap-3">
        <h1
          className={cn(
            "min-w-0 truncate font-sans text-4xl font-extrabold tracking-tight",
            collapsed && "text-xl",
          )}
        >
          {name}
        </h1>
        {shelf === undefined ? null : (
          <span className="shrink-0 text-sm text-fg-muted tabular-nums">
            {shelf.item_count} books
          </span>
        )}
      </div>
      {!collapsed ? (
        <div className="flex flex-wrap items-center justify-between gap-4 empty:hidden">
          <ReadingStateBar data={readingCounts} />
          <NeedsAttentionCounts data={attentionCounts} />
        </div>
      ) : null}
    </header>
  );
}
