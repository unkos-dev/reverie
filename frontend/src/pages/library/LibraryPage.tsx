/**
 * Production `/library` page.
 *
 * One route, one query, two projections: filters, search, and sort live
 * in the URL and apply identically to the cover grid and the table; the
 * view switcher changes the projection, never the query. The toolbar is
 * view-neutral (search, view switcher, Filters trigger) with table-only
 * display controls (density, columns); active filters render as
 * removable chips under it.
 *
 * Two right-side overlays share one slot: the filter drawer and the
 * book-detail drawer are mutually exclusive by construction, with the
 * Sheet primitive supplying the shared scrim, Escape handling, and focus
 * return. Selection is table-only and clears whenever the query identity
 * changes: acting on rows the current query no longer shows would be
 * worse than re-selecting.
 */
import { useSuspenseInfiniteQuery, type InfiniteData } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import {
  lazy,
  Suspense,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactElement,
} from "react";
import { Link } from "react-router";

import {
  listBooks,
  parseSortParam,
  type BookListItem,
  type BookListResponse,
  type SortLevelParam,
} from "@/api";
import { CoverArtwork } from "@/components/CoverArtwork";
import { Atmosphere } from "@/components/library/Atmosphere";
import { BookmarkRibbon } from "@/components/library/BookmarkRibbon";
import {
  filterResetToken,
  makeFilterClear,
  makeFilterCommit,
  type EditTokens,
} from "@/components/library/filter-commit";
import { LibraryMasthead } from "@/components/library/LibraryMasthead";
import { sortStackSummary } from "@/components/library/sort-summary";
import { FilterRail, type SeriesFacetOption } from "@/components/shell/FilterRail";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuthMe } from "@/hooks/useAuthMe";
import { useCinematicMode } from "@/hooks/useCinematicMode";
import { useLiveSearchParams } from "@/lib/hooks/use-live-search-params";
import { queryKeys } from "@/lib/query/keys";

import {
  applySortToSearchParams,
  FILTER_PARAM_KEYS,
  filterStateToParams,
  hasActiveFilterState,
  paramsFromSearch,
  parseFilterParams,
  PURGE_ONLY_PARAM_KEYS,
  viewFromSearch,
  type LibraryView,
} from "@/routes/library-params";

import { BatchBar } from "./BatchBar";
import { BookDetailDrawer } from "./BookDetailDrawer";
import { readDisplayPreferences, writeDisplayPreferences, type Density } from "./display-storage";
import { FilterChips } from "./FilterChips";
import { LibraryToolbar } from "./LibraryToolbar";
import { TableChunkBoundary } from "./TableChunkBoundary";
import { readViewCookie, writeViewCookie } from "./view-cookie";

/**
 * The table view carries the grid vendor chunk, so it loads lazily: cover
 * browsing never pays its bundle cost, and the chunk stays out of the
 * route's critical path.
 */
const LibraryTableView = lazy(() =>
  import("./table/LibraryTableView").then((m) => ({
    default: m.LibraryTableView,
  })),
);

/** The library's one overlay slot: filters or one book's details, never both. */
type OverlayState = "filters" | { detail: string } | null;

/**
 * Top-level page component. The `<Suspense>` boundary catches the
 * initial fetch (already prefetched by the loader, but the boundary
 * is required by `useSuspenseInfiniteQuery` semantics in failure /
 * cache-miss scenarios).
 */
export function LibraryPage(): ReactElement {
  return (
    <Suspense fallback={<LibrarySkeleton />}>
      <LibraryContent />
    </Suspense>
  );
}

function LibraryContent(): ReactElement {
  // Single URL write authority for the route: the rail's commits and the
  // page's own writers (header sort, clear-all, view toggle, quick search,
  // chip removal) all go through `applyParams`, so two writes landing in
  // one frame cannot clobber each other (see the hook's docstring).
  const { searchParams, applyParams, clearGen } = useLiveSearchParams();
  // Drives cinematic mode via the document `data-cinematic` attribute (CSS
  // reads it); the boolean return is unused — visibility is CSS-only.
  useCinematicMode();
  // One edit-token ref per page: the toolbar quick search and the rail's
  // sections share the draft-survival protocol through it (filter-commit.ts).
  const lastEdit = useRef<EditTokens | null>(null);
  const [overlay, setOverlay] = useState<OverlayState>(null);
  // The overlays open from state, not a Radix Trigger, so Radix has no
  // trigger to restore focus to on close; the opener is captured here and
  // restored via onCloseAutoFocus instead (WCAG 2.4.3).
  const overlayReturnFocus = useRef<HTMLElement | null>(null);
  // Escape = abandon: bumped when the filter drawer closes via Escape so
  // pending rail drafts die at fire time; scrim and close-button closes
  // leave it alone and pending drafts flush on unmount (see FilterRail).
  const railCancelGen = useRef(0);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(new Set());
  const [displayPrefs] = useState(readDisplayPreferences);
  const [density, setDensity] = useState<Density>(displayPrefs.density ?? "comfortable");
  const [hiddenColumns, setHiddenColumns] = useState<ReadonlySet<string>>(
    new Set(displayPrefs.hiddenColumns ?? []),
  );

  // URL param is canonical (shareable); the cookie only supplies the default
  // when the param is absent, so a chosen view survives leaving and returning.
  const viewMode: LibraryView = viewFromSearch(searchParams) ?? readViewCookie() ?? "grid";
  const params = paramsFromSearch(searchParams);
  const filterState = parseFilterParams(searchParams);
  // Strip cursor from the cache key — Load more is driven by react-query's pageParam.
  const cacheParams = { ...params };
  delete cacheParams.cursor;

  // Selection lifecycle: a changed query identity (filters, search, sort)
  // or projection switch clears it — the selected rows may no longer be in
  // the result set, and a batch action on hidden rows is worse than
  // re-selecting. Paging appends to the same query and keeps it. Render-
  // phase state adjustment, the compiler-accepted alternative to a sync
  // effect.
  const queryToken = (() => {
    const token = new URLSearchParams(searchParams);
    token.delete("cursor");
    token.sort();
    return token.toString();
  })();
  const [syncedQueryToken, setSyncedQueryToken] = useState(queryToken);
  if (queryToken !== syncedQueryToken) {
    setSyncedQueryToken(queryToken);
    if (selectedIds.size > 0) setSelectedIds(new Set());
  }

  const {
    data,
    error: fetchNextPageError,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  } = useSuspenseInfiniteQuery<
    BookListResponse,
    Error,
    InfiniteData<BookListResponse, string | undefined>,
    ReturnType<typeof queryKeys.books.list>,
    string | undefined
  >({
    queryKey: queryKeys.books.list(cacheParams),
    queryFn: ({ signal, pageParam }) =>
      listBooks(
        pageParam === undefined ? cacheParams : { ...cacheParams, cursor: pageParam },
        signal,
      ),
    initialPageParam: undefined,
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });

  // The user-facing Load-more error is rendered below; this routes the raw
  // error to the console too (QueryCache.onError only forwards 401s), so a
  // 500 / parse failure leaves a developer breadcrumb.
  useEffect(() => {
    if (isFetchNextPageError)
      console.error("[LibraryContent] failed to load the next page", fetchNextPageError);
  }, [isFetchNextPageError, fetchNextPageError]);

  const items: BookListItem[] = data.pages.flatMap((p) => p.items);

  function setView(next: LibraryView): void {
    writeViewCookie(next);
    applyParams((params) => {
      if (next === "grid") params.delete("view");
      else params.set("view", next);
      return params;
    });
  }

  /** Table-header sort writes the same `?sort=` contract as the rail's sort section. */
  function setSortFromTable(levels: readonly SortLevelParam[]): void {
    applyParams((params) => applySortToSearchParams(params, levels));
  }

  function clearAllFilters(): void {
    lastEdit.current = null;
    applyParams(
      (params) => {
        for (const key of FILTER_PARAM_KEYS) params.delete(key);
        for (const key of PURGE_ONLY_PARAM_KEYS) params.delete(key);
        params.delete("cursor");
        return params;
      },
      // Pending drafts (quick search, rail sections) must die with the
      // clear, or a due debounce could re-write a filter the user removed.
      { clears: true },
    );
  }

  function setDensityPref(next: Density): void {
    setDensity(next);
    writeDisplayPreferences({ density: next });
  }

  function toggleColumn(key: string, hidden: boolean): void {
    setHiddenColumns((current) => {
      const next = new Set(current);
      if (hidden) next.add(key);
      else next.delete(key);
      writeDisplayPreferences({ hiddenColumns: [...next] });
      return next;
    });
  }

  function resetColumns(): void {
    setHiddenColumns(new Set());
    writeDisplayPreferences({ hiddenColumns: [] });
  }

  // Counted on the wire projection, not raw URL keys: a param the codec
  // rejects (malformed number, unknown status token) is never sent, so it
  // must not light the badge or flip the empty state either.
  const activeFilterCount = Object.keys(filterStateToParams(filterState)).length;

  function openOverlay(next: Exclude<OverlayState, null>): void {
    overlayReturnFocus.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setOverlay(next);
  }

  function restoreOverlayFocus(event: Event): void {
    // A detached opener (row refetched away while the drawer was open)
    // must fall through to Radix's default rather than stranding focus
    // on <body> behind a prevented default.
    const opener = overlayReturnFocus.current;
    if (opener === null || !opener.isConnected) return;
    event.preventDefault();
    opener.focus();
  }

  /** Empty states first, then one branch per projection. */
  function renderBooks(): ReactElement {
    if (items.length === 0) {
      if (hasActiveFilterState(filterState))
        return <FilteredEmptyState onClear={clearAllFilters} />;
      return <EmptyState />;
    }
    if (viewMode === "grid") return <BookGrid items={items} />;
    return (
      <TableChunkBoundary
        onFallbackToGrid={() => {
          setView("grid");
        }}
      >
        <Suspense fallback={<Skeleton className="h-96 w-full" />}>
          <LibraryTableView
            items={items}
            sort={parseSortParam(params.sort ?? "")}
            onSortChange={setSortFromTable}
            hasNextPage={hasNextPage}
            isFetchingNextPage={isFetchingNextPage}
            isFetchNextPageError={isFetchNextPageError}
            onLoadMore={() => {
              void fetchNextPage();
            }}
            listQueryKey={queryKeys.books.list(cacheParams)}
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
              header-driven sorts unannounced. */}
          <p className="sr-only" aria-live="polite">
            {sortStackSummary(parseSortParam(params.sort ?? ""))}
          </p>
          <LibraryToolbar
            view={viewMode}
            onViewChange={setView}
            searchValue={filterState.q ?? ""}
            searchResetToken={filterResetToken(filterState)}
            lastEdit={lastEdit}
            clearGen={clearGen}
            onSearchCommit={(q) => {
              // Built inside the handler: the writer closes over the ref and
              // may only touch it outside render (react-compiler contract).
              makeFilterCommit(applyParams, lastEdit)((current) => ({ ...current, q }));
            }}
            filtersOpen={overlay === "filters"}
            activeFilterCount={activeFilterCount}
            onOpenFilters={() => {
              openOverlay("filters");
            }}
            density={density}
            onDensityChange={setDensityPref}
            hiddenColumns={hiddenColumns}
            onToggleColumn={toggleColumn}
            onResetColumns={resetColumns}
          />
          <FilterChips
            filters={filterState}
            seriesNames={seriesById}
            onRemove={(patch) => {
              makeFilterClear(applyParams, lastEdit)(patch);
            }}
            onClearAll={clearAllFilters}
          />
          <Separator className="mb-8" />

          {renderBooks()}

          {viewMode === "table" ? (
            <BatchBar
              selectedIds={selectedIds}
              hasMorePages={hasNextPage}
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
          {viewMode !== "table" && hasNextPage ? (
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
                disabled={isFetchingNextPage}
                onClick={() => {
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
      <Sheet
        open={overlay === "filters"}
        onOpenChange={(open) => {
          setOverlay(open ? "filters" : null);
        }}
      >
        <SheetContent
          id="library-filter-drawer"
          side="right"
          aria-describedby={undefined}
          onCloseAutoFocus={restoreOverlayFocus}
          onEscapeKeyDown={() => {
            railCancelGen.current += 1;
          }}
          className="w-[340px] max-w-[100vw] overflow-y-auto p-6"
        >
          <SheetHeader className="mb-2 p-0">
            <SheetTitle className="font-display text-2xl font-medium">Refine</SheetTitle>
          </SheetHeader>
          <FilterRail
            seriesOptions={seriesOptions}
            applyParams={applyParams}
            clearGen={clearGen}
            lastEdit={lastEdit}
            cancelGen={railCancelGen}
          />
        </SheetContent>
      </Sheet>
      <BookDetailDrawer
        bookId={typeof overlay === "object" && overlay !== null ? overlay.detail : null}
        onOpenChange={(open) => {
          if (!open) setOverlay(null);
        }}
        onCloseAutoFocus={restoreOverlayFocus}
      />
    </>
  );
}

interface BookGridProps {
  items: BookListItem[];
}

function BookGrid({ items }: BookGridProps): ReactElement {
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
}

interface BookCardProps {
  book: BookListItem;
}

function BookCard({ book }: BookCardProps): ReactElement {
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
        viewTransition
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

/**
 * True-empty state: the library genuinely holds no books. An admin can
 * reach ingestion in one hop, so offer the link; non-admin readers (adult
 * and child alike) see the holding copy only and get no dead-end action.
 */
function EmptyState(): ReactElement {
  const { data: me } = useAuthMe();
  return (
    <div className="border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed py-16 text-center">
      <p className="font-display text-fg mb-2 text-xl font-semibold">No books yet</p>
      <p className="text-sm">Once ingestion completes, books appear here.</p>
      {me?.role === "admin" ? (
        <Button asChild variant="outline" size="sm" className="mt-6">
          <Link to="/admin/dashboard">Go to ingestion</Link>
        </Button>
      ) : null}
    </div>
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
function FilteredEmptyState({ onClear }: FilteredEmptyStateProps): ReactElement {
  return (
    <div className="border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed py-16 text-center">
      <p className="font-display text-fg mb-2 text-xl font-semibold">
        No books match these filters
      </p>
      <p className="text-sm">Try removing a filter to widen your search.</p>
      <Button type="button" variant="outline" size="sm" className="mt-6" onClick={onClear}>
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
