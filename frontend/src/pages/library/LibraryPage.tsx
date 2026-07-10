/**
 * Production `/library` page.
 *
 * Mirrors the visual contract of the dev hero (`/design/hero/library`)
 * — same tokens, same grid spacing, same typographic hierarchy — but
 * sources data from the real `/api/v1/books` endpoint via react-query's
 * `useSuspenseInfiniteQuery`. The route loader has already seeded
 * page 1 into the cache; this component subscribes and renders.
 *
 * Renders the editorial masthead and ambient atmosphere over the browse
 * column — the filter rail (the sole filter and sort editor), the read-only
 * filter summary with the rail toggle, the view-mode toggle, and Load-more
 * pagination over the fetched pages.
 */
import { useSuspenseInfiniteQuery, type InfiniteData } from "@tanstack/react-query";
import { LayoutGrid, List, Loader2, Table2 } from "lucide-react";
import { lazy, Suspense, useEffect, useState, type CSSProperties, type ReactElement } from "react";
import { Link, useSearchParams } from "react-router";

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
import { FilterSummary } from "@/components/library/FilterSummary";
import { LibraryMasthead } from "@/components/library/LibraryMasthead";
import { BrowseLayout } from "@/components/shell/BrowseLayout";
import { FilterRail, type SeriesFacetOption } from "@/components/shell/FilterRail";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuthMe } from "@/hooks/useAuthMe";
import { useCinematicMode } from "@/hooks/useCinematicMode";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import { queryKeys } from "@/lib/query/keys";

import {
  applySortToSearchParams,
  FILTER_PARAM_KEYS,
  paramsFromSearch,
  parseFilterParams,
  viewFromSearch,
  type LibraryView,
} from "@/routes/library-params";

import { readRailCollapsed, writeRailCollapsed } from "./rail-storage";
import { TableChunkBoundary } from "./TableChunkBoundary";
import { readViewCookie, writeViewCookie } from "./view-cookie";

/**
 * The table view carries the grid vendor chunk, so it loads lazily: grid and
 * list browsing never pay its bundle cost, and the chunk stays out of the
 * route's critical path.
 */
const LibraryTableView = lazy(() =>
  import("./table/LibraryTableView").then((m) => ({ default: m.LibraryTableView })),
);

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
  const [searchParams, setSearchParams] = useSearchParams();
  // Drives cinematic mode via the document `data-cinematic` attribute (CSS
  // reads it); the boolean return is unused — visibility is CSS-only.
  useCinematicMode();
  // Rail visibility splits by width: ≥1280px toggles the persisted column
  // collapse, below that the toggle drives the transient sheet.
  const isDesktop = useMediaQuery("(min-width: 1280px)");
  const [railCollapsed, setRailCollapsed] = useState(() => readRailCollapsed() ?? false);
  const [sheetOpen, setSheetOpen] = useState(false);
  function toggleRail(): void {
    if (isDesktop) {
      setRailCollapsed((collapsed) => {
        writeRailCollapsed(!collapsed);
        return !collapsed;
      });
      return;
    }
    setSheetOpen((open) => !open);
  }
  // URL param is canonical (shareable); the cookie only supplies the default
  // when the param is absent, so a chosen view survives leaving and returning.
  const viewMode: LibraryView = viewFromSearch(searchParams) ?? readViewCookie() ?? "grid";
  const params = paramsFromSearch(searchParams);
  const filterState = parseFilterParams(searchParams);
  // Strip cursor from the cache key — Load more is driven by react-query's pageParam.
  const cacheParams = { ...params };
  delete cacheParams.cursor;

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
    const updated = new URLSearchParams(searchParams);
    if (next === "grid") updated.delete("view");
    else updated.set("view", next);
    setSearchParams(updated, { replace: true });
  }

  /** Table-header sort writes the same `?sort=` contract as the rail's sort section. */
  function setSortFromTable(levels: readonly SortLevelParam[]): void {
    setSearchParams(applySortToSearchParams(searchParams, levels), { replace: true });
  }

  function clearAllFilters(): void {
    const updated = new URLSearchParams(searchParams);
    for (const key of FILTER_PARAM_KEYS) updated.delete(key);
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
  }

  /** Empty states first, then one branch per view mode. */
  function renderBooks(): ReactElement {
    if (items.length === 0) {
      if (hasActiveFilters(searchParams)) return <FilteredEmptyState onClear={clearAllFilters} />;
      return <EmptyState />;
    }
    if (viewMode === "grid") return <BookGrid items={items} />;
    if (viewMode === "list") return <BookList items={items} />;
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
      {/* Raise the whole browse layout — rail included — above the fixed
          atmosphere layers. The rail renders outside `children`, so a
          content-only stacking context leaves it under `.lib-grain` (z-1). */}
      <div className="relative z-[2]">
        <BrowseLayout
          rail={<FilterRail seriesOptions={seriesOptions} />}
          railCollapsed={railCollapsed}
          sheetOpen={sheetOpen}
          onSheetOpenChange={setSheetOpen}
        >
          {/* No max-width cap — the browse room uses the full column so
            ultrawide gets ~10 columns, not 4 stamps in a void (spec §5).
            The auto-fill clamp(170px,10vw,240px) bounds tile size. */}
          <div className="px-6 py-10 sm:px-10">
            <LibraryMasthead />
            <div data-chrome="" className="mb-6 flex flex-wrap items-center justify-between gap-4">
              {/* Always-visible read-only readout + rail toggle, every view
                  mode: state stays visible even with the rail collapsed. */}
              <FilterSummary
                filters={filterState}
                seriesNames={seriesById}
                railExpanded={isDesktop ? !railCollapsed : sheetOpen}
                onToggleRail={toggleRail}
              />
              <div className="flex flex-wrap items-center gap-2">
                <div
                  role="group"
                  aria-label="View mode"
                  className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
                >
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-pressed={viewMode === "grid"}
                    onClick={() => {
                      setView("grid");
                    }}
                    className={
                      viewMode === "grid" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""
                    }
                  >
                    <LayoutGrid className="size-4" aria-hidden="true" />
                    <span className="sr-only">Grid</span>
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-pressed={viewMode === "list"}
                    onClick={() => {
                      setView("list");
                    }}
                    className={
                      viewMode === "list" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""
                    }
                  >
                    <List className="size-4" aria-hidden="true" />
                    <span className="sr-only">List</span>
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-pressed={viewMode === "table"}
                    onClick={() => {
                      setView("table");
                    }}
                    className={
                      viewMode === "table" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""
                    }
                  >
                    <Table2 className="size-4" aria-hidden="true" />
                    <span className="sr-only">Table</span>
                  </Button>
                </div>
              </div>
            </div>
            <Separator className="mb-8" />

            {renderBooks()}

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
        </BrowseLayout>
      </div>
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

interface BookListProps {
  items: BookListItem[];
}

function BookList({ items }: BookListProps): ReactElement {
  return (
    <table data-testid="library-list" className="border-border w-full border-collapse text-sm">
      <thead className="text-fg-muted text-left text-xs uppercase tracking-wide">
        <tr>
          <th scope="col" className="py-2 pr-4 font-medium">
            Title
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            Author
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            Series
          </th>
          <th scope="col" className="py-2 pr-4 font-medium">
            ISBN
          </th>
        </tr>
      </thead>
      <tbody>
        {items.map((book) => (
          <tr key={book.id} className="border-border hover:bg-surface-1 border-t">
            <td className="py-3 pr-4">
              <Link to={`/b/${book.id}`} className="hover:text-accent text-fg font-medium">
                {book.title}
              </Link>
            </td>
            <td className="text-fg-muted py-3 pr-4">{book.authors.join(", ")}</td>
            <td className="text-fg-muted py-3 pr-4">
              {book.series
                ? `${book.series.name}${
                    book.series.position !== null ? ` · #${String(book.series.position)}` : ""
                  }`
                : "—"}
            </td>
            <td className="text-fg-muted py-3 pr-4 font-mono text-xs">{book.isbn_13 ?? "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
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

/**
 * True when any typed filter, vocabulary set, or quick search is active.
 * Drives the empty-state split: filters present means a zero-result set is
 * "filtered to nothing", not "library is empty". Every filter key is checked
 * with `getAll` so multi-value params (`?tag=a&tag=b`) count, and the set is
 * the shared {@link FILTER_PARAM_KEYS} so it cannot drift from the codec.
 */
function hasActiveFilters(search: URLSearchParams): boolean {
  return FILTER_PARAM_KEYS.some((key) => search.getAll(key).some((value) => value !== ""));
}

function LibrarySkeleton(): ReactElement {
  const PLACEHOLDERS = Array.from({ length: 12 }, (_, i) => i);
  return (
    <div className="px-6 py-10 sm:px-10" aria-busy="true">
      {/* Masthead placeholder — mirror LibraryMasthead's hero band, kicker,
          and gilt-title heights so the Suspense fallback reserves the same
          vertical space the loaded masthead occupies (avoids a CLS jump). */}
      <div className="mb-10">
        <Skeleton className="-mx-6 -mt-10 mb-8 h-[clamp(220px,30vh,340px)] sm:-mx-10" />
        <Skeleton className="mb-3 h-7 w-64" />
        <Skeleton className="h-[clamp(4rem,9.5vw,9rem)] w-72 max-w-full" />
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
