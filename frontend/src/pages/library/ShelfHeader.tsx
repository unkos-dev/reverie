import { graphql, useFragment } from "react-relay";
import type { ReactElement } from "react";

import { ATTENTION_FILTER_LABELS, type AttentionFilter } from "@/components/library/filter-summary";
import { Button } from "@/components/ui/button";
import type {
  ShelfHeader_summary$data,
  ShelfHeader_summary$key,
} from "@/lib/relay/__generated__/ShelfHeader_summary.graphql";
import type { ShelfHeader_shelves$key } from "@/lib/relay/__generated__/ShelfHeader_shelves.graphql";
import { cn } from "@/lib/utils";
import type { FilterState, StatusFilter } from "@/routes/library-params";

const summaryFragment = graphql`
  fragment ShelfHeader_summary on LibrarySummary @throwOnFieldError {
    totalCount
    readingStates {
      finished
      reading
      unread
    }
    needsAttention {
      toReview
      enrichmentFailed
      noIsbn
      noCover
    }
  }
`;

const shelvesFragment = graphql`
  fragment ShelfHeader_shelves on Shelf @relay(plural: true) @throwOnFieldError {
    id
    name
  }
`;

type Status = StatusFilter["any"][number];
type Attention = AttentionFilter | "noIsbn";
type ReadingCounts = ShelfHeader_summary$data["readingStates"];
type AttentionCounts = ShelfHeader_summary$data["needsAttention"];
type FilterControls = {
  filters: FilterState;
  onStatus: (status: Status) => void;
  onAttention: (attention: Attention) => void;
};

function ReadingStateBar({
  counts,
  status,
  onStatus,
}: Readonly<{
  counts: ReadingCounts;
  status: StatusFilter;
  onStatus: FilterControls["onStatus"];
}>): ReactElement {
  const entries = [
    { token: "finished", label: "Read", count: counts.finished, colour: "bg-fg-muted" },
    { token: "reading", label: "Reading", count: counts.reading, colour: "bg-accent" },
    { token: "unread", label: "Unread", count: counts.unread, colour: "bg-border-control" },
  ] as const;
  const isSelected = (token: Status): boolean =>
    status.any.length === 1 && status.any.includes(token) && status.none.length === 0;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
      <div className="relative flex h-6 w-75 max-w-full items-stretch gap-0.5 before:pointer-events-none before:absolute before:inset-x-0 before:top-1/2 before:h-1.5 before:-translate-y-1/2 before:rounded-full before:bg-surface-2">
        {entries
          .filter(({ count }) => count > 0)
          .map(({ token, label, count, colour }) => (
            <Button
              key={token}
              variant="ghost"
              size="xs"
              excludeFromTabOrder
              aria-label={`${label}, ${String(count)}`}
              aria-pressed={isSelected(token)}
              style={{ flexGrow: count }}
              className="relative min-w-1 basis-0 shrink border-0 px-0 aria-pressed:ring-1 aria-pressed:ring-accent-text"
              onPress={() => {
                onStatus(token);
              }}
            >
              <span aria-hidden="true" className={cn("h-1.5 w-full rounded-full", colour)} />
            </Button>
          ))}
      </div>
      <div className="flex flex-wrap gap-x-1">
        {entries.map(({ token, label, count, colour }) => (
          <Button
            key={token}
            variant="ghost"
            size="xs"
            aria-pressed={isSelected(token)}
            className="gap-1.5 px-1.5 tabular-nums aria-pressed:border-accent-text aria-pressed:text-fg"
            onPress={() => {
              onStatus(token);
            }}
          >
            <span aria-hidden="true" className={cn("size-1.5 rounded-full", colour)} />
            <span className="font-semibold text-fg">{count}</span> {label}
          </Button>
        ))}
      </div>
    </div>
  );
}

function NeedsAttentionCounts({
  counts,
  filters,
  onAttention,
}: Readonly<{
  counts: AttentionCounts;
  filters: FilterState;
  onAttention: FilterControls["onAttention"];
}>): ReactElement | null {
  const entries = [
    { token: "needsReview", count: counts.toReview, selected: filters.needsReview },
    {
      token: "enrichmentFailed",
      count: counts.enrichmentFailed,
      selected: filters.enrichmentFailed,
    },
    { token: "noIsbn", count: counts.noIsbn, selected: filters.isbn13.op === "empty" },
    { token: "noCover", count: counts.noCover, selected: filters.noCover },
  ] as const;
  const visible = entries.filter(({ count }) => count > 0);
  if (visible.length === 0) return null;
  return (
    <div className="ml-auto hidden flex-wrap items-center justify-end gap-1.5 sm:flex">
      <span className="mr-1 text-xs text-fg-muted">Needs attention</span>
      {visible.map(({ token, count, selected }) => (
        <Button
          key={token}
          variant="outline"
          size="xs"
          aria-pressed={selected === true}
          className="rounded-full bg-transparent text-fg-muted tabular-nums aria-pressed:border-accent-text aria-pressed:text-fg"
          onPress={() => {
            onAttention(token);
          }}
        >
          <span className="font-semibold text-fg">{count}</span>{" "}
          {token === "noIsbn" ? "no ISBN" : ATTENTION_FILTER_LABELS[token]}
        </Button>
      ))}
    </div>
  );
}

type Props = FilterControls & {
  shelves: ShelfHeader_shelves$key;
  selected: readonly string[];
  summary: ShelfHeader_summary$key | undefined;
};

export function ShelfHeader({
  shelves,
  selected,
  summary,
  filters,
  onStatus,
  onAttention,
}: Readonly<Props>): ReactElement {
  const labels = useFragment(shelvesFragment, shelves);
  const data = useFragment(summaryFragment, summary ?? null);
  const shelf = selected.length === 1 ? labels.find((item) => item.id === selected[0]) : undefined;
  const name =
    selected.length > 1 ? `${String(selected.length)} shelves` : (shelf?.name ?? "Library");
  return (
    <header className="flex flex-col gap-4 px-2 py-5">
      <div className="flex min-w-0 items-baseline gap-3">
        <h1 className="min-w-0 truncate font-sans text-4xl font-extrabold tracking-tight">
          {name}
        </h1>
        {data == null ? null : (
          <span className="shrink-0 text-sm text-fg-muted tabular-nums">
            {data.totalCount} books
          </span>
        )}
      </div>
      {data == null ? null : (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
          <ReadingStateBar
            counts={data.readingStates}
            status={filters.status}
            onStatus={onStatus}
          />
          <NeedsAttentionCounts
            counts={data.needsAttention}
            filters={filters}
            onAttention={onAttention}
          />
        </div>
      )}
    </header>
  );
}
