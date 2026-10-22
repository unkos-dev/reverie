/**
 * Active-filter chips row: one removable chip per filter condition, plus
 * a clear-all affordance. Replaces the read-only filter summary; the
 * chips are built from the same projection (`buildFilterSummary`), whose
 * segments now carry their own removal patch. Chip removal is a clearing
 * write (it must kill pending drafts), so the container hands down the
 * clear-path writer, not the commit-path one.
 */
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useEffect, type ReactElement } from "react";
import { Button, Tag, TagGroup, TagList } from "react-aria-components";

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
}: Readonly<FilterChipsProps>): ReactElement {
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

  return (
    <div data-testid="filter-chips" className="mb-4 flex flex-wrap items-center gap-1.5">
      <TagGroup
        aria-label="Active filters"
        onRemove={(keys) => {
          const removed = segments.filter((segment) => keys.has(segment.key));
          onRemove((current) => removed.reduce((next, segment) => segment.remove(next), current));
        }}
      >
        <TagList
          items={segments}
          className="text-fg-muted flex flex-wrap gap-1.5 text-xs outline-none data-focus-visible:ring-2 data-focus-visible:ring-accent"
          renderEmptyState={() => "No active filters."}
        >
          {(segment) => (
            <Tag
              id={segment.key}
              textValue={segment.text}
              className="border-border text-fg-muted inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs outline-none data-focus-visible:ring-2 data-focus-visible:ring-accent"
            >
              {segment.text}
              <Button
                slot="remove"
                className="hover:text-fg focus-visible:ring-accent flex min-h-6 min-w-6 items-center justify-center rounded-full focus-visible:outline-none focus-visible:ring-2"
              >
                <X className="size-3" aria-hidden="true" />
              </Button>
            </Tag>
          )}
        </TagList>
      </TagGroup>
      <Button
        isDisabled={segments.length === 0}
        onPress={onClearAll}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent px-1.5 py-1 font-mono text-[0.65rem] uppercase tracking-[0.08em] underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 data-disabled:opacity-50"
      >
        Clear all
      </Button>
    </div>
  );
}
