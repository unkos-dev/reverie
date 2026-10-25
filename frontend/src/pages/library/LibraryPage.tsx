import {
  useInfiniteQuery,
  infiniteQueryOptions,
  keepPreviousData,
  type InfiniteData,
} from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import {
  lazy,
  memo,
  Suspense,
  useDeferredValue,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type ReactElement,
} from "react";
import { Link } from "react-router";

import { listBooks, type BookListItem, type BookListResponse, type SortLevelParam } from "@/api";
import { CoverArtwork } from "@/components/CoverArtwork";
import { Atmosphere } from "@/components/library/Atmosphere";
import { BookmarkRibbon } from "@/components/library/BookmarkRibbon";
import { LibraryMasthead } from "@/components/library/LibraryMasthead";
import { sortStackSummary } from "@/components/library/sort-summary";
import { FilterRail, type SeriesFacetOption } from "@/components/shell/FilterRail";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { useCinematicMode } from "@/hooks/useCinematicMode";
import { useLibraryFilters } from "@/lib/hooks/use-library-filters";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import {
  filterStateToParams,
  hasActiveFilterState,
  type LibraryView,
} from "@/routes/library-params";

import { BatchBar } from "./BatchBar";
import { BookDetailDrawer } from "./BookDetailDrawer";
import { EmptyLibrary } from "./EmptyLibrary";
import { FilterChips } from "./FilterChips";
import { LibraryToolbar } from "./LibraryToolbar";
import { TableChunkBoundary } from "./TableChunkBoundary";
import { useLibraryPreferences } from "./use-library-preferences";

/**
 * The table view carries the grid vendor chunk, so it loads lazily: cover
 * browsing never pays its bundle cost, and the chunk stays out of the
 * route's critical path.
 */
const LibraryTableView = lazy(() =>
  import("./table/LibraryTableView").then((m) => ({ default: m.LibraryTableView })),
);

/** The library's one overlay slot: filters or one book's details, never both. */
type OverlayState = "filters" | { detail: string } | null;

type BookPages = InfiniteData<BookListResponse, string | undefined> & {
  originKey?: BooksListKey;
};

function listOptions(queryKey: BooksListKey) {
  return infiniteQueryOptions<BookListResponse, Error, BookPages, BooksListKey, string | undefined>(
    {
      queryKey,
      queryFn: ({ queryKey: [, , params], signal, pageParam }) =>
        listBooks(pageParam === undefined ? params : { ...params, cursor: pageParam }, signal),
      initialPageParam: undefined,
      getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
    },
  );
}

export function LibraryPage(): ReactElement {
  const owner = useLibraryFilters();
  const { filters: filterState, commitAll, clearAll, setView: setVisitView } = owner;
  // Drives cinematic mode via the document `data-cinematic` attribute (CSS
  // reads it); the boolean return is unused — visibility is CSS-only.
  useCinematicMode();
  const [overlay, setOverlay] = useState<OverlayState>(null);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(new Set());
  const preferences = useLibraryPreferences();
  const { density, hiddenColumns, sortOverride, sortLevels, setSortLevels } = preferences;

  const viewMode: LibraryView = owner.view ?? preferences.view;
  const params = filterStateToParams(filterState);
  const deferredSortOverride = useDeferredValue(sortOverride);
  const sortPending = deferredSortOverride !== sortOverride;
  if (deferredSortOverride !== "") params.sort = deferredSortOverride;
  const cacheParams = params;
  const listKey = queryKeys.books.list(cacheParams);
  const queryToken = JSON.stringify([filterState, sortOverride, viewMode]);

  const {
    data,
    error: fetchNextPageError,
    isPending,
    isError,
    isPlaceholderData,
    isFetching,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  } = useInfiniteQuery({
    ...listOptions(listKey),
    placeholderData: (previous, previousQuery) => {
      const retained = keepPreviousData(previous);
      return retained === undefined
        ? undefined
        : { ...retained, originKey: previousQuery?.queryKey };
    },
  });

  const displayedKey = data?.originKey ?? listKey;
  const retained = useInfiniteQuery({
    ...listOptions(displayedKey),
    enabled: isPlaceholderData,
    staleTime: Infinity,
  });
  const displayedData = isPlaceholderData ? retained.data : data;
  const selectionToken = JSON.stringify([queryToken, displayedKey]);
  const [syncedSelectionToken, setSyncedSelectionToken] = useState(selectionToken);
  if (selectionToken !== syncedSelectionToken) {
    setSyncedSelectionToken(selectionToken);
    if (selectedIds.size > 0) setSelectedIds(new Set());
  }

  // The user-facing Load-more error is rendered below; this routes the raw
  // error to the console too (QueryCache.onError only forwards 401s), so a
  // 500 / parse failure leaves a developer breadcrumb.
  useEffect(() => {
    if (isFetchNextPageError)
      console.error("[LibraryPage] failed to load the next page", fetchNextPageError);
  }, [isFetchNextPageError, fetchNextPageError]);

  const items: BookListItem[] = useMemo(
    () => displayedData?.pages.flatMap((p) => p.items) ?? [],
    [displayedData],
  );
  const updating = sortPending || isPlaceholderData || (isFetching && !isFetchingNextPage);
  const canLoadMore = hasNextPage && !isPlaceholderData;

  function setView(next: LibraryView): void {
    preferences.setView(next);
    setVisitView(next);
  }

  /**
   * The one sort intent handler: the table header and the rail's stack
   * editor both land here, and here alone. The gesture writes the reader's
   * preference directly; nothing infers sort intent from URL diffs, because
   * sort has no URL form. An empty stack is the reset gesture: it drops the
   * override and the display visibly becomes the installation stack.
   */
  function handleSortChange(levels: readonly SortLevelParam[]): void {
    setSortLevels(levels);
  }

  function toggleColumn(key: string, hidden: boolean): void {
    const next = new Set(hiddenColumns);
    if (hidden) next.add(key);
    else next.delete(key);
    preferences.setHiddenColumns(next);
  }

  const activeFilterCount = Object.keys(filterStateToParams(filterState)).length;

  function openOverlay(next: Exclude<OverlayState, null>): void {
    setOverlay(next);
  }

  /** Empty states first, then one branch per projection. */
  function renderBooks(): ReactElement {
    if (items.length === 0) {
      if (hasActiveFilterState(filterState)) return <FilteredEmptyState onClear={clearAll} />;
      return <EmptyLibrary />;
    }
    if (viewMode === "grid") return <BookGrid items={items} />;
    return (
      <TableChunkBoundary
        onFallbackToGrid={() => {
          setVisitView("grid");
        }}
      >
        <Suspense fallback={<Skeleton className="h-96 w-full" />}>
          <LibraryTableView
            items={items}
            sort={sortLevels}
            onSortChange={handleSortChange}
            hasNextPage={canLoadMore}
            isFetchingNextPage={isFetchingNextPage}
            isFetchNextPageError={isFetchNextPageError}
            onLoadMore={() => {
              void fetchNextPage();
            }}
            listQueryKey={displayedKey}
            density={density}
            hiddenColumns={hiddenColumns}
            selectedIds={selectedIds}
            onSelectionChange={setSelectedIds}
            onRowActivate={(book) => {
              openOverlay({ detail: book.id });
            }}
          />
        </Suspense>
      </TableChunkBoundary>
    );
  }

  // Facet options derive from the loaded pages — `SeriesRef` carries
  // the id the backend filter wants; authors have no stable id, so they
  // group by display name only.
  const seriesById = new Map<string, string>();
  for (const book of items) {
    if (book.series !== null) seriesById.set(book.series.id, book.series.name);
  }
  const seriesOptions: SeriesFacetOption[] = [...seriesById]
    .map(([id, name]) => ({ id, name }))
    .sort((a, b) => a.name.localeCompare(b.name));

  if (isPending) return <LibrarySkeleton />;
  if (isError && !isFetchNextPageError) throw fetchNextPageError;

  return (
    <>
      <Atmosphere />
      <BookmarkRibbon />
      {/* Always rendered; `.cinema-hint` fades in/out purely via the
          `[data-cinematic="on"]` opacity rule. Gating it on the boolean
          would unmount it in the same commit that clears the attribute,
          so the CSS fade-out could never paint. `aria-hidden` because it's
          a visual-only affordance — opacity:0 still exposes it to AT, so
          without this it would be announced on every load. */}
      <p
        className="cinema-hint font-mono text-fg-muted text-xs uppercase tracking-[0.2em]"
        aria-hidden="true"
      >
        Cinematic mode · press F to exit
      </p>
      {/* Raise the browse content above the fixed atmosphere layers. */}
      <div className="relative z-[2]">
        <div className="px-6 py-10 sm:px-10">
          <LibraryMasthead />
          {/* Sort announcements stay mounted here: the filter drawer (which
              carries the sort section) unmounts when closed, and an
              unmounted live region is silent, which would leave
              header-driven sorts unannounced. Announces the effective stack
              (immediate, not deferred): intent is voiced at gesture time,
              while the rows carry `aria-busy` until the new order lands. */}
          <p className="sr-only" aria-live="polite">
            {sortStackSummary(sortLevels)}
          </p>
          <LibraryToolbar
            query={filterState.q ?? ""}
            onQueryChange={(q) => {
              commitAll((current) => ({ ...current, q }));
            }}
            view={viewMode}
            onViewChange={setView}
            filtersOpen={overlay === "filters"}
            activeFilterCount={activeFilterCount}
            onOpenFilters={() => {
              openOverlay("filters");
            }}
            density={density}
            onDensityChange={preferences.setDensity}
            hiddenColumns={hiddenColumns}
            onToggleColumn={toggleColumn}
            canResetColumns={preferences.canResetColumns}
            onResetColumns={preferences.resetColumns}
          />
          <FilterChips
            filters={filterState}
            seriesNames={seriesById}
            onRemove={commitAll}
            onClearAll={clearAll}
          />
          <Separator className="mb-8" />

          {/* `aria-busy` while a sort gesture's re-sorted page is loading:
              the deferred query key keeps the previous rows visible, and
              this is the stale-content marker for that window. */}
          {updating ? <p className="text-fg-muted mb-3 text-sm">Updating results…</p> : null}
          <div aria-busy={updating || undefined} className={updating ? "opacity-60" : ""}>
            {renderBooks()}
          </div>

          {viewMode === "table" ? (
            <BatchBar
              selectedIds={selectedIds}
              hasMorePages={canLoadMore}
              onClearSelection={() => {
                setSelectedIds(new Set());
              }}
              onCompleted={(succeededIds) => {
                setSelectedIds((current) => {
                  const next = new Set(current);
                  for (const id of succeededIds) next.delete(id);
                  return next;
                });
              }}
            />
          ) : null}

          {/* Table mode owns its paging UI (loading, error, Load-more,
              end-of-list) inside LibraryTableView; rendering this block
              there too would announce the same failure twice and offer two
              Retry controls. */}
          {viewMode !== "table" && canLoadMore ? (
            <div className="mt-10 flex flex-col items-center gap-3">
              {/* A failed `fetchNextPage` keeps the loaded pages on screen; the
                  error is hue-less (One-Accent rule — the danger hue is reserved
                  for unrecoverable errors, and this one is retryable) and carried
                  by copy + the Retry control. `role="alert"` announces it. */}
              {isFetchNextPageError ? (
                <p role="alert" className="text-fg-muted text-sm">
                  Couldn&apos;t load more books. Check your connection and try again.
                </p>
              ) : null}
              <Button
                type="button"
                variant="outline"
                isDisabled={isFetchingNextPage}
                onPress={() => {
                  void fetchNextPage();
                }}
              >
                {isFetchingNextPage ? (
                  <>
                    <Loader2 className="mr-2 size-4 animate-spin" aria-hidden="true" />
                    Loading…
                  </>
                ) : isFetchNextPageError ? (
                  "Retry"
                ) : (
                  "Load more"
                )}
              </Button>
            </div>
          ) : null}
          <footer className="border-border text-fg-faint mt-16 border-t pt-6 text-center font-mono text-[0.68rem] uppercase tracking-[0.18em]">
            Reverie · MMXXVI · Set in Author, Satoshi and JetBrains Mono
          </footer>
        </div>
      </div>

      {/* The two right-side overlays share the slot above; opening either
          closes the other by construction. */}

      <SheetContent
        isOpen={overlay === "filters"}
        onOpenChange={(open) => {
          setOverlay(open ? "filters" : null);
        }}

        id="library-filter-drawer"
        side="right"
        aria-describedby={undefined}
        className="max-w-[100vw] overflow-y-auto p-6 data-[side=right]:w-[340px]"
      >
        <SheetHeader className="mb-2 p-0">
          <SheetTitle className="font-display text-2xl font-medium">Refine</SheetTitle>
        </SheetHeader>
        <FilterRail
          owner={owner}
          showContradiction={!updating && !isError}
          seriesOptions={seriesOptions}
          sortLevels={sortLevels}
          sortInherited={preferences.sortInherited}
          onSortChange={handleSortChange}
        />
      </SheetContent>

      <BookDetailDrawer
        bookId={typeof overlay === "object" && overlay !== null ? overlay.detail : null}
        onOpenChange={(open) => {
          if (!open) setOverlay(null);
        }}
      />
    </>
  );
}

interface BookGridProps {
  items: BookListItem[];
}

const BookGrid = memo(function BookGrid({ items }: BookGridProps): ReactElement {
  return (
    <ul
      data-testid="library-grid"
      className="grid gap-x-6 gap-y-8 [grid-template-columns:repeat(auto-fill,minmax(clamp(170px,10vw,240px),1fr))]"
    >
      {items.map((book, index) => {
        // Stagger index is the one dynamic value utilities can't carry;
        // typed as an intersection so no `as` cast is needed.
        const staggerStyle: CSSProperties & { "--tile-index": number } = {
          "--tile-index": index,
        };
        return (
          <li key={book.id} className="tile-in" style={staggerStyle}>
            <BookCard book={book} />
          </li>
        );
      })}
    </ul>
  );
});

interface BookCardProps {
  book: BookListItem;
}

function BookCard({ book }: Readonly<BookCardProps>): ReactElement {
  // Spine fallback active when there is no art to fall back FROM, or
  // the art failed to load. The floating series badge renders only
  // over real cover art — over a typographic spine it collides with
  // the composition's own type.
  const [coverFailed, setCoverFailed] = useState(false);
  const usesSpine = book.cover_url === "" || coverFailed;
  const seriesLabel =
    book.series && book.series.position !== null
      ? `${book.series.name} · #${String(book.series.position)}`
      : (book.series?.name ?? null);
  // Delay the z-index drop on hover-out so the card stays above its
  // neighbours for the full 200ms lift-down — otherwise z resets instantly
  // and the accent glow is clipped mid-animation.
  return (
    <article className="group relative z-0 transition-[z-index] delay-200 duration-0 hover:z-10 hover:delay-0 focus-within:z-10 focus-within:delay-0">
      <Link
        to={`/b/${book.id}`}
        className="focus-visible:ring-accent focus-visible:ring-offset-canvas flex flex-col gap-3 rounded-md transition-transform duration-200 ease-out hover:-translate-y-2 hover:scale-[1.04] focus-visible:-translate-y-2 focus-visible:scale-[1.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 motion-reduce:transform-none"
      >
        <div className="border-border group-hover:border-border-strong bg-surface-1 group-hover:shadow-[0_14px_32px_-16px_var(--accent-glow)] group-focus-within:shadow-[0_14px_32px_-16px_var(--accent-glow)] relative aspect-[2/3] overflow-hidden border transition-[border-color,box-shadow] duration-200 motion-reduce:transition-none">
          {usesSpine ? (
            <CoverArtwork bookId={book.id} title={book.title} authors={book.authors} />
          ) : (
            <img
              src={book.cover_url}
              alt={`Cover of ${book.title}`}
              loading="lazy"
              decoding="async"
              onError={() => {
                setCoverFailed(true);
              }}
              className="size-full object-cover"
            />
          )}
          {seriesLabel !== null && !usesSpine ? (
            <span className="bg-canvas/85 text-fg border-border absolute left-2 top-2 rounded-sm border px-2 py-1 text-[0.62rem] uppercase tracking-[0.14em] backdrop-blur-sm">
              {seriesLabel}
            </span>
          ) : null}
        </div>
        <div className="flex flex-col gap-1">
          <h3 className="font-display text-fg line-clamp-2 text-sm font-medium leading-tight">
            {book.title}
          </h3>
          <p className="text-fg-muted line-clamp-1 text-xs leading-tight">
            {book.authors.join(", ")}
          </p>
        </div>
      </Link>
    </article>
  );
}

interface FilteredEmptyStateProps {
  /** Drops every filter param, returning the browse to the full library. */
  onClear: () => void;
}

/**
 * Filtered-empty state: the library has books, but the active filters
 * exclude all of them. Distinct from {@link EmptyState} (a genuinely
 * empty library) — same shell, accurate copy, plus a one-click escape so
 * an over-narrow filter is never a dead-end.
 */
function FilteredEmptyState({ onClear }: Readonly<FilteredEmptyStateProps>): ReactElement {
  return (
    <div className="border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed py-16 text-center">
      <p className="font-display text-fg mb-2 text-xl font-semibold">
        No books match these filters
      </p>
      <p className="text-sm">Try removing a filter to widen your search.</p>
      <Button type="button" variant="outline" size="sm" className="mt-6" onPress={onClear}>
        Clear all filters
      </Button>
    </div>
  );
}

function LibrarySkeleton(): ReactElement {
  const PLACEHOLDERS = Array.from({ length: 12 }, (_, i) => i);
  return (
    <div className="px-6 py-10 sm:px-10" aria-busy="true">
      {/* Masthead placeholder — mirror the hero band height (title renders
          inside the band now) so the Suspense fallback reserves the same
          vertical space the loaded masthead occupies (avoids a CLS jump). */}
      <div className="mb-8">
        <Skeleton className="-mx-6 -mt-10 h-[clamp(220px,10vw,340px)] sm:-mx-10" />
      </div>
      {/* Toolbar placeholder: search field + control cluster. */}
      <div className="mb-4 flex items-center gap-2">
        <Skeleton className="h-9 flex-1" />
        <Skeleton className="h-9 w-24" />
        <Skeleton className="h-9 w-24" />
      </div>
      <Separator className="mb-8" />
      {/* Same auto-fill expression as the loaded BookGrid — a fixed
          breakpoint ladder here would change column count when data
          arrives, producing a visible layout jump. */}
      <div className="grid gap-x-6 gap-y-8 [grid-template-columns:repeat(auto-fill,minmax(clamp(170px,10vw,240px),1fr))]">
        {PLACEHOLDERS.map((i) => (
          <div key={i} className="flex flex-col gap-3">
            <Skeleton className="aspect-[2/3] w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-3 w-1/2" />
          </div>
        ))}
      </div>
    </div>
  );
}
