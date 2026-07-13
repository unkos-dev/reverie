/**
 * Active-filter chips row: one removable chip per filter condition, plus
 * a clear-all affordance. Replaces the read-only filter summary; the
 * chips are built from the same projection (`buildFilterSummary`), whose
 * segments now carry their own removal patch. Chip removal is a clearing
 * write (it must kill pending drafts), so the container hands down the
 * clear-path writer, not the commit-path one.
 *
 * Renders nothing when no filters are active, so the row never shows
 * "All records"-style prose over an unfiltered grid.
 */
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useEffect, type ReactElement } from "react";

import { listShelves, type Shelf } from "@/api";
import {
  buildFilterSummary,
  shortId,
  type SummarySegment,
} from "@/components/library/filter-summary";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { queryKeys } from "@/lib/query/keys";
import type { FilterState } from "@/routes/library-params";

type FilterChipsProps = {
  filters: FilterState;
  /** Series id to display name, from the page's loaded rows. */
  seriesNames: ReadonlyMap<string, string>;
  /** Clearing write: applies a segment's removal patch. */
  onRemove: (patch: SummarySegment["remove"]) => void;
  onClearAll: () => void;
};

export function FilterChips({
  filters,
  seriesNames,
  onRemove,
  onClearAll,
}: FilterChipsProps): ReactElement | null {
  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);
  const hasShelfFilter = filters.shelf !== undefined;
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
    // Nothing to resolve without an active shelf filter; skip the fetch.
    enabled: hasShelfFilter,
  });
  useEffect(() => {
    if (shelvesError) console.error("[FilterChips] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);

  const segments = buildFilterSummary(filters, {
    authorLabel: authorNames.labelFor,
    shelfName: (id) => shelves?.find((shelf) => shelf.id === id)?.name ?? shortId(id),
    seriesName: (id) => seriesNames.get(id) ?? shortId(id),
  });

  if (segments.length === 0) return null;

  return (
    <div
      data-testid="filter-chips"
      className="mb-4 flex flex-wrap items-center gap-1.5"
      aria-live="polite"
    >
      {segments.map((segment) => (
        <span
          key={segment.key}
          className="border-border text-fg-muted inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs"
        >
          {segment.text}
          <button
            type="button"
            aria-label={`Remove filter: ${segment.text}`}
            onClick={() => {
              onRemove(segment.remove);
            }}
            className="hover:text-fg focus-visible:ring-accent flex min-h-4 min-w-4 items-center justify-center rounded-full focus-visible:outline-none focus-visible:ring-2"
          >
            <X className="size-3" aria-hidden="true" />
          </button>
        </span>
      ))}
      <button
        type="button"
        onClick={onClearAll}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent px-1.5 py-1 font-mono text-[0.65rem] uppercase tracking-[0.08em] underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2"
      >
        Clear all
      </button>
    </div>
  );
}
