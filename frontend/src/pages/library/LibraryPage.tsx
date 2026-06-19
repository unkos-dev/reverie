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
 * column — the filter rail, the shelf/sort/active-filter controls, the
 * grid/list toggle, and Load-more pagination over the fetched pages.
 */
import { useQuery, useSuspenseInfiniteQuery, type InfiniteData } from "@tanstack/react-query";
import { Filter, LayoutGrid, List, Loader2, X } from "lucide-react";
import { Suspense, useEffect, useState, type CSSProperties, type ReactElement } from "react";
import { Link, useSearchParams } from "react-router";

import {
  listBooks,
  listShelves,
  type BookListItem,
  type BookListResponse,
  type ListSort,
  type Shelf,
} from "@/api";
import { CoverArtwork } from "@/components/CoverArtwork";
import { Atmosphere } from "@/components/library/Atmosphere";
import { BookmarkRibbon } from "@/components/library/BookmarkRibbon";
import { LibraryMasthead } from "@/components/library/LibraryMasthead";
import { BrowseLayout } from "@/components/shell/BrowseLayout";
import { FilterRail, type SeriesFacetOption } from "@/components/shell/FilterRail";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useCinematicMode } from "@/hooks/useCinematicMode";
import { queryKeys } from "@/lib/query/keys";

import { paramsFromSearch } from "@/routes/library-params";

type ViewMode = "grid" | "list";

const SORT_OPTIONS: { value: ListSort; label: string }[] = [
  { value: "recent", label: "Recent" },
  { value: "title", label: "Title" },
  { value: "author", label: "Author" },
];

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
  const cinematic = useCinematicMode();
  const viewMode: ViewMode = searchParams.get("view") === "list" ? "list" : "grid";
  const params = paramsFromSearch(searchParams);
  // Strip cursor from the cache key — Load more is driven by react-query's pageParam.
  const cacheParams = { ...params };
  delete cacheParams.cursor;

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useSuspenseInfiniteQuery<
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

  const items: BookListItem[] = data.pages.flatMap((p) => p.items);

  function setView(next: ViewMode): void {
    const updated = new URLSearchParams(searchParams);
    if (next === "grid") updated.delete("view");
    else updated.set("view", next);
    setSearchParams(updated, { replace: true });
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
  const authorNames = [...new Set(items.flatMap((b) => b.authors))].sort((a, b) =>
    a.localeCompare(b),
  );

  return (
    <>
      <Atmosphere />
      <BookmarkRibbon />
      {/* Cinematic mode dims the chrome; this hint shows how to exit. */}
      {cinematic ? (
        <p className="cinema-hint font-mono text-fg-muted text-xs uppercase tracking-[0.2em]">
          Cinematic mode · press F to exit
        </p>
      ) : null}
      {/* Raise the whole browse layout — rail included — above the fixed
          atmosphere layers. The rail renders outside `children`, so a
          content-only stacking context leaves it under `.lib-grain` (z-1). */}
      <div className="relative z-[2]">
        <BrowseLayout rail={<FilterRail seriesOptions={seriesOptions} authorNames={authorNames} />}>
          {/* No max-width cap — the browse room uses the full column so
            ultrawide gets ~10 columns, not 4 stamps in a void (spec §5).
            The auto-fill clamp(170px,10vw,240px) bounds tile size. */}
          <div className="px-6 py-10 sm:px-10">
            <LibraryMasthead />
            <div data-chrome="" className="mb-6 flex flex-wrap items-center justify-between gap-4">
              <div className="flex flex-wrap items-center gap-2">
                <ShelfPickerButton searchParams={searchParams} setSearchParams={setSearchParams} />
                <ActiveFilterChips searchParams={searchParams} setSearchParams={setSearchParams} />
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <SortMenu searchParams={searchParams} setSearchParams={setSearchParams} />
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
                </div>
              </div>
            </div>
            <Separator className="mb-8" />

            {items.length === 0 ? (
              <EmptyState />
            ) : viewMode === "grid" ? (
              <BookGrid items={items} />
            ) : (
              <BookList items={items} />
            )}

            {hasNextPage ? (
              <div className="mt-10 flex justify-center">
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

interface SortMenuProps {
  searchParams: URLSearchParams;
  setSearchParams: (next: URLSearchParams, options?: { replace?: boolean }) => void;
}

/**
 * Sort control — a real `<button>` menu (spec §9.3), wired to the
 * existing `?sort=` contract (`recent` is the backend default, so it
 * clears the param instead of writing it).
 */
function SortMenu({ searchParams, setSearchParams }: SortMenuProps): ReactElement {
  const raw = searchParams.get("sort");
  const active = SORT_OPTIONS.find((o) => o.value === raw) ?? { value: "recent", label: "Recent" };

  function setSort(value: string): void {
    const opt = SORT_OPTIONS.find((o) => o.value === value);
    if (opt === undefined) return;
    const updated = new URLSearchParams(searchParams);
    if (opt.value === "recent") updated.delete("sort");
    else updated.set("sort", opt.value);
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button type="button" variant="outline" size="sm">
          Sort: {active.label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup value={active.value} onValueChange={setSort}>
          {SORT_OPTIONS.map((opt) => (
            <DropdownMenuRadioItem key={opt.value} value={opt.value}>
              {opt.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
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
        <div className="border-border group-hover:border-border-strong bg-surface-1 group-hover:shadow-[0_18px_44px_-12px_var(--accent)] group-focus-within:shadow-[0_18px_44px_-12px_var(--accent)] relative aspect-[2/3] overflow-hidden border transition-[border-color,box-shadow] duration-200 motion-reduce:transition-none">
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

function EmptyState(): ReactElement {
  return (
    <div className="border-border text-fg-muted flex min-h-[40vh] flex-col items-center justify-center rounded-md border border-dashed py-16 text-center">
      <p className="font-display text-fg mb-2 text-xl font-semibold">No books yet</p>
      <p className="text-sm">Once ingestion completes, books appear here.</p>
    </div>
  );
}

/**
 * Active-filter chip row (11b). Renders one chip per URL search-param
 * filter (`?author=`, `?series=`, `?shelf=`) with a clear (`×`)
 * affordance — clicking removes that key and triggers a react-query
 * refetch via the route params hook. `aria-pressed` on the chip itself
 * signals the filter is on (matches the hero-page chip pattern in
 * `/design/hero/library`).
 *
 * Source-list affordances (browse-by-shelf / browse-by-series chips
 * with an "add" action) land alongside the GET `/api/v1/series/{id}` and
 * shelves CRUD endpoints in sub-phase 11d. Here we own the *clear*
 * half so navigation arrivals (CommandPalette, series link from a book
 * card) always have a way out.
 */
interface ActiveFilterChipsProps {
  searchParams: URLSearchParams;
  setSearchParams: (next: URLSearchParams, options?: { replace?: boolean }) => void;
}

type ChipKey = "author" | "series" | "shelf" | "tag";

interface ActiveChip {
  /** Unique chip id (`"tag:scifi"` for tag chips, plain key otherwise). */
  id: string;
  /** Which URL param the chip controls. */
  key: ChipKey;
  /** Human-readable chip label. */
  label: string;
  /** Tag value the chip clears, for multi-value `?tag=` chips. */
  tagValue?: string;
}

function ActiveFilterChips({
  searchParams,
  setSearchParams,
}: ActiveFilterChipsProps): ReactElement | null {
  // 11d: resolve shelf id → name via the shelves cache when present
  // so the chip reads "shelf: Wishlist" instead of "shelf: a1b2c3d4…".
  // `useQuery` with `enabled: false` would block the first render; we
  // unconditionally subscribe to the cache and let the picker mutation
  // populate it on first use.
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  // "(unknown)" is acceptable UI degradation; the failure behind it
  // must still reach the console (QueryCache.onError only routes 401s).
  useEffect(() => {
    if (shelvesError) console.error("[ActiveFilterChips] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);
  const shelfNameFor = (id: string): string => {
    if (shelvesError) return "(unknown)";
    return shelves?.find((s) => s.id === id)?.name ?? shortId(id);
  };

  const filters: ActiveChip[] = [];
  for (const key of ["author", "series", "shelf"] as const) {
    const value = searchParams.get(key);
    if (value !== null && value !== "") {
      const label = key === "shelf" ? `Shelf: ${shelfNameFor(value)}` : `${key}: ${shortId(value)}`;
      filters.push({ id: key, key, label });
    }
  }
  // `?tag=a&tag=b` repeats — one chip per value so the user can clear
  // them independently. Mirrors the backend's multi-value AND-match
  // semantics; clearing one tag removes only that name from the filter.
  for (const tagValue of searchParams.getAll("tag")) {
    if (tagValue === "") continue;
    filters.push({
      id: `tag:${tagValue}`,
      key: "tag",
      label: `tag: ${tagValue}`,
      tagValue,
    });
  }
  if (filters.length === 0) return null;

  function clear(chip: ActiveChip): void {
    const updated = new URLSearchParams(searchParams);
    if (chip.key === "tag" && chip.tagValue !== undefined) {
      const remaining = updated.getAll("tag").filter((v) => v !== chip.tagValue);
      updated.delete("tag");
      for (const t of remaining) updated.append("tag", t);
    } else {
      updated.delete(chip.key);
    }
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
  }

  return (
    <div className="mb-6 flex flex-wrap items-center gap-2" data-testid="active-filters">
      {filters.map((chip) => (
        <Button
          key={chip.id}
          type="button"
          variant="outline"
          size="sm"
          aria-pressed
          onClick={() => {
            clear(chip);
          }}
          className="border-border bg-accent-soft text-fg hover:bg-accent-soft/80 h-7 gap-1 rounded-full px-3 font-mono text-xs"
        >
          {chip.label}
          <X className="size-3" aria-hidden="true" />
          <span className="sr-only">
            Clear {chip.key}
            {chip.tagValue !== undefined ? ` ${chip.tagValue}` : ""} filter
          </span>
        </Button>
      ))}
    </div>
  );
}

/** Render a UUID compactly (first 8 chars) for chip labels. */
function shortId(value: string): string {
  return value.length > 10 ? `${value.slice(0, 8)}…` : value;
}

interface ShelfPickerButtonProps {
  searchParams: URLSearchParams;
  setSearchParams: (next: URLSearchParams, options?: { replace?: boolean }) => void;
}

/**
 * Picker affordance for the `?shelf=` filter (11d). Opens a Command
 * popover listing the caller's shelves; selecting one sets the URL
 * param and triggers a refetch via the existing react-router data
 * mode. Author / series pickers are blocked on missing `GET
 * /api/v1/authors` and `GET /api/v1/series` list endpoints (recorded as a
 * follow-up in the 11d report).
 */
function ShelfPickerButton({
  searchParams,
  setSearchParams,
}: ShelfPickerButtonProps): ReactElement {
  const [open, setOpen] = useState(false);
  const {
    data: shelves,
    isLoading,
    isError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  function pick(shelfId: string): void {
    const updated = new URLSearchParams(searchParams);
    updated.set("shelf", shelfId);
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
    setOpen(false);
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 gap-1 rounded-full px-3 text-xs"
          aria-label="Filter by shelf"
        >
          <Filter className="size-3" aria-hidden="true" />
          Shelf
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="p-0">
        <Command>
          <CommandInput placeholder="Filter by shelf…" />
          <CommandList>
            <CommandEmpty>
              {isLoading
                ? "Loading shelves…"
                : isError
                  ? "Could not load shelves."
                  : "No shelves yet."}
            </CommandEmpty>
            <CommandGroup heading="Shelves">
              {(shelves ?? []).map((shelf) => (
                <CommandItem
                  key={shelf.id}
                  value={`${shelf.name} ${shelf.id}`}
                  onSelect={() => {
                    pick(shelf.id);
                  }}
                >
                  {shelf.name}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
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
