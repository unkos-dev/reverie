/**
 * Read-only masthead summary of the active library filters, plus the rail
 * toggle. Rendered in every view mode, including with the rail collapsed, so
 * no condition can narrow the collection invisibly. The summary never edits
 * filter state: the rail owns every filter and sort gesture, and this
 * component's only interactive element is the show/hide toggle.
 *
 * Vocabulary families summarise as counts (a single token shows its resolved
 * name) so the line stays constant-size however many conditions are active;
 * shelf and series ids resolve to display names, degrading to a shortened id
 * when the name is not resolvable from the loaded data.
 */
import { useQuery } from "@tanstack/react-query";
import { SlidersHorizontal } from "lucide-react";
import { useEffect, type ReactElement } from "react";

import { listShelves, type Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { queryKeys } from "@/lib/query/keys";
import type { FilterState } from "@/routes/library-params";

import { buildFilterSummary, shortId } from "./filter-summary";

type FilterSummaryProps = {
  filters: FilterState;
  /** Series id to display name, from the page's loaded rows. */
  seriesNames: ReadonlyMap<string, string>;
  /** Whether the rail is currently visible (column expanded, or sheet open). */
  railExpanded: boolean;
  onToggleRail: () => void;
};

/** Masthead filter readout + rail toggle; see the module docstring. */
export function FilterSummary({
  filters,
  seriesNames,
  railExpanded,
  onToggleRail,
}: FilterSummaryProps): ReactElement {
  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
    // Nothing to resolve without an active shelf filter; skip the fetch.
    enabled: filters.shelf !== undefined,
  });
  useEffect(() => {
    if (shelvesError) console.error("[FilterSummary] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);

  const segments = buildFilterSummary(filters, {
    authorLabel: authorNames.labelFor,
    shelfName: (id) => shelves?.find((shelf) => shelf.id === id)?.name ?? shortId(id),
    seriesName: (id) => seriesNames.get(id) ?? shortId(id),
  });

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2" data-testid="filter-summary">
      <Button
        type="button"
        variant="outline"
        size="sm"
        aria-expanded={railExpanded}
        onClick={onToggleRail}
      >
        <SlidersHorizontal className="size-4" aria-hidden="true" />
        Filters
        {segments.length > 0 ? (
          <span className="bg-accent-soft text-fg rounded-full px-1.5 py-0.5 text-[0.65rem] leading-none">
            {segments.length}
          </span>
        ) : null}
      </Button>
      {segments.length > 0 ? (
        <p className="text-fg-muted min-w-0 flex-1 truncate text-xs">
          {segments.map((segment) => segment.text).join(" · ")}
        </p>
      ) : null}
    </div>
  );
}
