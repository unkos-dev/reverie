import { decodeGraphQLId } from "@/api/graphql";
import { useShelves } from "@/api/shelves";
/**
 * Active-filter chips row: one removable chip per filter condition, plus
 * a clear-all affordance. Replaces the read-only filter summary; the
 * chips are built from the same projection (`buildFilterSummary`), whose
 * segments now carry their own removal patch. Chip removal is a clearing
 * write (it must kill pending drafts), so the container hands down the
 * clear-path writer, not the commit-path one.
 */
import { X } from "lucide-react";
import { useEffect, type ReactElement } from "react";
import { Button, Tag, TagGroup, TagList } from "react-aria-components";

import {
  buildFilterSummary,
  shortId,
  type SummarySegment,
} from "@/components/library/filter-summary";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { useSeriesLabels } from "@/lib/hooks/use-series-labels";
import type { FilterState } from "@/routes/library-params";

type FilterChipsProps = {
  filters: FilterState;
  /** Clearing write: applies a segment's removal patch. */
  onRemove: (patch: SummarySegment["remove"]) => void;
  onClearAll: () => void;
};

export function FilterChips({
  filters,
  onRemove,
  onClearAll,
}: Readonly<FilterChipsProps>): ReactElement {
  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);
  const seriesNames = useSeriesLabels(filters.series);
  const hasShelfFilter = filters.shelf.length > 0;
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useShelves(undefined, hasShelfFilter);
  useEffect(() => {
    if (shelvesError) console.error("[FilterChips] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);

  const segments = buildFilterSummary(filters, {
    authorLabel: authorNames.labelFor,
    shelfName: (id) =>
      shelves.find((shelf) => shelf.id === id)?.name ?? shortId(decodeGraphQLId("Shelf", id)),
    seriesName: seriesNames.labelFor,
  });

  return (
    <div data-testid="filter-chips" className="flex flex-wrap items-center gap-1.5">
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
              className="border-border text-fg-muted inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs outline-none data-focus-visible:ring-2 data-focus-visible:ring-accent"
            >
              <span>{segment.text.slice(0, segment.text.indexOf(" ")).replace(/:$/, "")}</span>
              <span className="text-fg font-medium">
                {segment.text.slice(segment.text.indexOf(" ") + 1)}
              </span>
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
        className="text-fg-muted hover:text-fg focus-visible:ring-accent px-1.5 py-1 text-sm underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 data-disabled:opacity-50"
      >
        Clear all
      </Button>
    </div>
  );
}
