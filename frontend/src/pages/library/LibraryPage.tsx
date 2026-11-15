import {
  useInfiniteQuery,
  useQuery,
  useMutation,
  infiniteQueryOptions,
  keepPreviousData,
  skipToken,
  useQueryClient,
  type InfiniteData,
  type QueryClient,
} from "@tanstack/react-query";
import { Circle, CircleCheck, Loader2, Star, Undo2 } from "lucide-react";
import {
  lazy,
  Suspense,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useRef,
  useMemo,
  useState,
  type ReactNode,
  type ReactElement,
} from "react";
import { matchPath, useNavigate } from "react-router";

import {
  ApiError,
  addShelfItem,
  listShelves,
  type Shelf,
  listBooks,
  type BookListItem,
  type BookListResponse,
  type SortLevelParam,
} from "@/api";
import { z } from "zod";
import { GridList, GridListItem } from "@/components/ui/grid-list";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import { Link } from "@/components/ui/link";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import "./library.css";

import { CoverArtwork } from "@/components/CoverArtwork";
import { sortStackSummary } from "@/components/library/sort-summary";
import { FilterRail } from "@/components/shell/FilterRail";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { useLibraryFilters } from "@/lib/hooks/use-library-filters";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import {
  countActiveFilters,
  filterStateToParams,
  hasActiveFilterState,
  isLibraryRequestEligible,
  hasOverLimitSelection,
  type LibraryView,
} from "@/routes/library-params";

import { useCellEdit, type UseCellEditResult } from "./table/useCellEdit";
import { GridShortcutsDialog, GridShortcutsTrigger } from "./table/GridShortcutsDialog";
import { FieldError } from "@/components/ui/field";
import { BatchBar } from "./BatchBar";
import { ShelfHeader } from "./ShelfHeader";
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
const LibraryTable = lazy(() =>
  import("./table/LibraryTable").then((m) => ({ default: m.LibraryTable })),
);

const COVER_WIDTH_KEY = "reverie:cover-width";
const CoverWidthSchema = z.coerce.number().int().min(120).max(300);

function saveCoverWidth(width: number): void {
  try {
    localStorage.setItem(COVER_WIDTH_KEY, String(width));
  } catch (error) {
    console.error("[LibraryPage] cover width could not be saved", error);
  }
}

type BookPages = InfiniteData<BookListResponse, string | undefined> & {
  originKey?: BooksListKey;
};

function listOptions(queryKey: BooksListKey, eligible: boolean, client: QueryClient) {
  const retryPolicy =
    client.getQueryDefaults(queryKey).retry ?? client.getDefaultOptions().queries?.retry;
  function retry(count: number, error: Error): boolean {
    if (error instanceof ApiError && error.status === 422) return false;
    if (typeof retryPolicy === "function") return retryPolicy(count, error);
    if (typeof retryPolicy === "boolean") return retryPolicy;
    return count < (retryPolicy ?? 3);
  }
  return infiniteQueryOptions<BookListResponse, Error, BookPages, BooksListKey, string | undefined>(
    {
      queryKey,
      queryFn: eligible
        ? ({ queryKey: [, , params], signal, pageParam }) =>
            listBooks(pageParam === undefined ? params : { ...params, cursor: pageParam }, signal)
        : skipToken,
      initialPageParam: undefined,
      retry,
      getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
    },
  );
}

function useLibraryBooks(listKey: BooksListKey, eligible: boolean, sortPending: boolean) {
  const client = useQueryClient();
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
    ...listOptions(listKey, eligible, client),
    placeholderData: (previous, previousQuery) => {
      const retained = keepPreviousData(previous);
      return retained === undefined
        ? undefined
        : { ...retained, originKey: previousQuery?.queryKey };
    },
  });

  const displayedKey = data?.originKey ?? listKey;
  const retained = useInfiniteQuery({
    ...listOptions(displayedKey, eligible, client),
    enabled: eligible && isPlaceholderData,
    staleTime: Infinity,
  });
  const displayedData = isPlaceholderData ? retained.data : data;
  const rejected = fetchNextPageError instanceof ApiError && fetchNextPageError.status === 422;
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
  const blocked = !eligible || rejected;
  const updating =
    !blocked &&
    (sortPending ||
      isPlaceholderData ||
      (isFetching && !isFetchingNextPage) ||
      retained.isRefetching);
  const canLoadMore = !blocked && hasNextPage && !isPlaceholderData;
  let blockedMessage: string | undefined;
  if (!eligible)
    blockedMessage = hasOverLimitSelection(listKey[2])
      ? "Results are paused. Choose 20 or fewer values in each filter mode, or clear the filters."
      : "Results are paused. Shorten the search or filter text to 200 characters or fewer, or clear the filters.";
  else if (rejected)
    blockedMessage = "These filters could not be used. Change or clear them to try again.";

  if (isError && !isFetchNextPageError && !rejected && eligible) throw fetchNextPageError;

  return {
    items,
    displayedKey,
    blocked,
    blockedMessage,
    updating,
    canLoadMore,
    loading: eligible && isPending,
    showContradiction: !blocked && !updating && !isError,
    fetchNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  };
}

function CellEditOutcomes({ edits }: { edits: UseCellEditResult }): ReactElement | null {
  const editFeedback = edits.feedback;
  return editFeedback.length > 0 || edits.undoOutcome !== null ? (
    <section aria-label="Edit outcomes" className="mb-4 space-y-3">
      {edits.undoOutcome !== null ? (
        <p role="status" className="text-fg-muted text-sm">
          {edits.undoOutcome}
        </p>
      ) : null}
      {editFeedback.map((feedback) => (
        <div key={feedback.id} className="space-y-2">
          <FieldError>
            {feedback.field} for “{feedback.bookTitle}”: {feedback.message}
          </FieldError>
          <p className="text-fg-muted break-words text-sm">Submitted value: {feedback.submitted}</p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            isPending={feedback.reloading}
            onPress={feedback.reload}
          >
            Reload
          </Button>
        </div>
      ))}
    </section>
  ) : null;
}

function TableEditControls({ edits }: { edits: UseCellEditResult }): ReactElement {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [undoFocused, setUndoFocused] = useState(false);
  const helpRef = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => {
    if (!edits.undoAvailable && !edits.undoPending && undoFocused) helpRef.current?.focus();
  }, [edits.undoAvailable, edits.undoPending, undoFocused]);
  return (
    <div className="flex items-center justify-end gap-2" {...edits.keyboardProps}>
      <Button
        onFocusChange={setUndoFocused}
        type="button"
        variant="outline"
        size="sm"
        onPress={edits.undo}
        isPending={edits.undoPending}
        isDisabled={!edits.undoAvailable && !edits.undoPending}
      >
        {edits.undoLabel === undefined ? null : <Undo2 aria-hidden="true" className="size-4" />}
        {edits.undoLabel === undefined ? "Undo last edit" : `Undo ${edits.undoLabel}`}
      </Button>
      <GridShortcutsTrigger buttonRef={helpRef} onOpenChange={setShortcutsOpen} />
      <GridShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
    </div>
  );
}

export function LibraryPage(): ReactElement {
  const owner = useLibraryFilters();
  const { filters: filterState, commitAll, clearAll, setView: setVisitView } = owner;
  const [filtersOpen, setFiltersOpen] = useState(false);
  const filtersBeside = useMediaQuery("(min-width: 1600px)");
  const filterButtonRef = useRef<HTMLButtonElement>(null);
  const [coverWidth, setCoverWidth] = useState(() => {
    try {
      const stored = CoverWidthSchema.safeParse(localStorage.getItem(COVER_WIDTH_KEY));
      return stored.success ? stored.data : 180;
    } catch {
      return 180;
    }
  });
  function closeFilters(): void {
    setFiltersOpen(false);
    filterButtonRef.current?.focus();
  }
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(new Set());
  const queryClient = useQueryClient();
  const {
    data: shelves,
    isError: shelvesError,
    error: shelvesQueryError,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  useEffect(() => {
    if (shelvesError) console.error("[LibraryPage] shelves fetch failed", shelvesQueryError);
  }, [shelvesError, shelvesQueryError]);

  const batch = useMutation({
    mutationFn: async ({
      shelf,
      ids,
    }: {
      shelf: Shelf;
      ids: readonly string[];
      retry?: boolean;
    }) => {
      const succeeded: string[] = [];
      const failed: string[] = [];
      for (const id of ids) {
        try {
          await addShelfItem(shelf.id, id);
          succeeded.push(id);
        } catch (error) {
          failed.push(id);
          console.error("[LibraryPage] add to shelf failed", { id, error });
        }
      }
      return { succeeded, failed };
    },
    onSuccess: ({ succeeded }) => {
      if (succeeded.length > 0)
        void queryClient.invalidateQueries({ queryKey: queryKeys.shelves.all });
      setSelectedIds((current) => {
        const next = new Set(current);
        for (const id of succeeded) next.delete(id);
        return next;
      });
    },
  });

  const [hasRenderedControls, setHasRenderedControls] = useState(false);
  const preferences = useLibraryPreferences();
  const { density, hiddenColumns, sortOverride, sortLevels, defaultSortLevels, setSortLevels } =
    preferences;

  const viewMode: LibraryView = owner.view ?? preferences.view;
  const params = filterStateToParams(filterState);
  const deferredSortOverride = useDeferredValue(sortOverride);
  const sortPending = deferredSortOverride !== sortOverride;
  if (deferredSortOverride !== "") params.sort = deferredSortOverride;
  const cacheParams = params;
  const listKey = queryKeys.books.list(cacheParams);
  const queryToken = JSON.stringify([filterState, sortOverride, viewMode]);

  const eligible =
    matchPath("/library", window.location.pathname) !== null && isLibraryRequestEligible(params);
  const {
    items,
    displayedKey,
    blockedMessage,
    updating,
    canLoadMore,
    loading,
    showContradiction,
    fetchNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  } = useLibraryBooks(listKey, eligible, sortPending);
  const edits = useCellEdit({ listKey: displayedKey, enabled: viewMode === "table" });
  const selectionToken = JSON.stringify([queryToken, displayedKey]);
  const [syncedSelectionToken, setSyncedSelectionToken] = useState(selectionToken);
  if (selectionToken !== syncedSelectionToken) {
    setSyncedSelectionToken(selectionToken);
    if (selectedIds.size > 0) setSelectedIds(new Set());
  }

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

  const activeFilterCount = countActiveFilters(filterState);

  if (!loading && !hasRenderedControls) setHasRenderedControls(true);
  if (loading && !hasRenderedControls) return <LibrarySkeleton />;

  return (
    <>
      <div className="lg:h-[calc(100dvh-86px)]">
        <div className="flex h-full min-h-0 flex-col px-3 pb-3 sm:px-5">
          <ShelfHeader shelves={shelves ?? []} selected={filterState.shelf} />
          {/* Sort announcements stay outside the filter editor so hidden controls do not silence them. */}
          <p className="sr-only" aria-live="polite">
            {sortStackSummary(sortLevels)}
          </p>
          <LibraryToolbar
            filters={
              <FilterChips filters={filterState} onRemove={commitAll} onClearAll={clearAll} />
            }
            editControls={viewMode === "table" ? <TableEditControls edits={edits} /> : null}
            loadedCount={items.length}
            hasMorePages={canLoadMore}
            query={filterState.q ?? ""}
            onQueryChange={(q) => {
              commitAll((current) => ({ ...current, q }));
            }}
            view={viewMode}
            onViewChange={setView}
            filtersOpen={filtersOpen}
            filterButtonRef={filterButtonRef}
            coverWidth={coverWidth}
            onCoverWidthChange={setCoverWidth}
            onCoverWidthChangeEnd={saveCoverWidth}
            sortLevels={sortLevels}
            onSortChange={handleSortChange}
            activeFilterCount={activeFilterCount}
            onOpenFilters={() => {
              setFiltersOpen((open) => !open);
            }}
            density={density}
            onDensityChange={preferences.setDensity}
            hiddenColumns={hiddenColumns}
            onToggleColumn={toggleColumn}
            canResetColumns={preferences.canResetColumns}
            onResetColumns={preferences.resetColumns}
          />
          <Separator className="mb-2" />

          <div
            className="library-results min-h-0 flex-1"
            data-panel-open={filtersBeside && filtersOpen}
          >
            <div className="flex min-h-0 min-w-0 flex-col">
              <CellEditOutcomes edits={edits} />
              <LibraryBooks
                items={items}
                filtered={hasActiveFilterState(filterState)}
                onClear={clearAll}
                viewMode={viewMode}
                coverWidth={coverWidth}
                selectedIds={selectedIds}
                onSelectionChange={setSelectedIds}
                blockedMessage={blockedMessage}
                loading={loading}
                updating={updating}
              >
                <TableChunkBoundary
                  onFallbackToGrid={() => {
                    setVisitView("grid");
                  }}
                >
                  <Suspense fallback={<Skeleton className="h-96 w-full" />}>
                    <LibraryTable
                      items={items}
                      sort={sortLevels}
                      defaultSort={defaultSortLevels}
                      onSortChange={handleSortChange}
                      hasNextPage={canLoadMore}
                      isFetchingNextPage={isFetchingNextPage}
                      isFetchNextPageError={isFetchNextPageError}
                      onLoadMore={() => {
                        void fetchNextPage();
                      }}
                      edits={edits}
                      density={density}
                      hiddenColumns={hiddenColumns}
                      onHideColumn={(key) => {
                        toggleColumn(key, true);
                      }}
                      selectedIds={selectedIds}
                      onSelectionChange={setSelectedIds}
                    />
                  </Suspense>
                </TableChunkBoundary>
              </LibraryBooks>

              {viewMode !== "table" && canLoadMore ? (
                <LibraryPagination
                  isFetching={isFetchingNextPage}
                  isError={isFetchNextPageError}
                  onLoadMore={() => {
                    void fetchNextPage();
                  }}
                />
              ) : null}
            </div>
            {filtersBeside ? (
              <div className="library-filter-slot" inert={!filtersOpen}>
                <FilterRail
                  owner={owner}
                  showContradiction={showContradiction}
                  sortLevels={sortLevels}
                  sortInherited={preferences.sortInherited}
                  onSortChange={handleSortChange}
                  onClose={closeFilters}
                />
              </div>
            ) : null}
          </div>
          <BatchBar
            selectedIds={selectedIds}
            hasMorePages={canLoadMore}
            onClearSelection={() => {
              setSelectedIds(new Set());
            }}
            shelves={shelves}
            shelvesError={shelvesError}
            batch={batch}
          />
        </div>
      </div>

      {!filtersBeside ? (
        <SheetContent
          isOpen={filtersOpen}
          onOpenChange={setFiltersOpen}
          side="right"
          showCloseButton={false}
          aria-describedby={undefined}
          className="max-w-[100vw] p-3 data-[side=right]:w-[340px]"
        >
          <SheetHeader className="sr-only">
            <SheetTitle>Filters</SheetTitle>
          </SheetHeader>
          <FilterRail
            owner={owner}
            showContradiction={showContradiction}
            sortLevels={sortLevels}
            sortInherited={preferences.sortInherited}
            onSortChange={handleSortChange}
            onClose={closeFilters}
          />
        </SheetContent>
      ) : null}
    </>
  );
}

type LibraryBooksProps = {
  items: BookListItem[];
  filtered: boolean;
  onClear: () => void;
  viewMode: LibraryView;
  coverWidth: number;
  selectedIds: ReadonlySet<string>;
  onSelectionChange: (ids: ReadonlySet<string>) => void;
  blockedMessage: string | undefined;
  loading: boolean;
  updating: boolean;
  children: ReactNode;
};

function LibraryBooks({
  items,
  filtered,
  onClear,
  viewMode,
  coverWidth,
  selectedIds,
  onSelectionChange,
  blockedMessage,
  loading,
  updating,
  children,
}: Readonly<LibraryBooksProps>): ReactElement {
  if (blockedMessage !== undefined)
    return (
      <p role="alert" className="text-fg-muted text-sm">
        {blockedMessage}
      </p>
    );
  let books = children;
  if (loading || (updating && items.length === 0)) books = <Skeleton className="h-96 w-full" />;
  else if (items.length === 0)
    books = filtered ? <FilteredEmptyState onClear={onClear} /> : <EmptyLibrary />;
  else if (viewMode === "grid")
    books = (
      <BookGrid
        items={items}
        coverWidth={coverWidth}
        selectedIds={selectedIds}
        onSelectionChange={onSelectionChange}
      />
    );
  return (
    <>
      {updating ? <p className="text-fg-muted mb-3 text-sm">Updating results…</p> : null}
      <div
        aria-busy={updating || undefined}
        className={`min-h-0 flex-1 ${viewMode === "grid" ? "overflow-y-auto overflow-x-hidden p-1" : ""} ${updating ? "opacity-60" : ""}`}
      >
        {books}
      </div>
    </>
  );
}

type LibraryPaginationProps = {
  isFetching: boolean;
  isError: boolean;
  onLoadMore: () => void;
};

function LibraryPagination({
  isFetching,
  isError,
  onLoadMore,
}: Readonly<LibraryPaginationProps>): ReactElement {
  const idleLabel = isError ? "Retry" : "Load more";
  return (
    <div className="mt-10 flex flex-col items-center gap-3">
      {/* Retryable paging errors reserve the danger hue for unrecoverable failures. */}
      {isError ? (
        <p role="alert" className="text-fg-muted text-sm">
          Couldn&apos;t load more books. Check your connection and try again.
        </p>
      ) : null}
      <Button type="button" variant="outline" isDisabled={isFetching} onPress={onLoadMore}>
        {isFetching ? (
          <>
            <Loader2 className="mr-2 size-4 animate-spin" aria-hidden="true" />
            Loading…
          </>
        ) : (
          idleLabel
        )}
      </Button>
    </div>
  );
}

type BookGridProps = {
  items: BookListItem[];
  coverWidth: number;
  selectedIds: ReadonlySet<string>;
  onSelectionChange: (ids: ReadonlySet<string>) => void;
};

function BookGrid({
  items,
  coverWidth,
  selectedIds,
  onSelectionChange,
}: Readonly<BookGridProps>): ReactElement {
  const navigate = useNavigate();
  return (
    <GridList
      aria-label="Books"
      data-testid="library-grid"
      layout="grid"
      items={items}
      selectionMode="multiple"
      selectionBehavior="toggle"
      selectedKeys={selectedIds}
      onSelectionChange={(keys) => {
        onSelectionChange(
          new Set(keys === "all" ? items.map((book) => book.id) : [...keys].map(String)),
        );
      }}
      onAction={(key) => {
        void navigate(`/b/${String(key)}`);
      }}
      style={{ gridTemplateColumns: `repeat(auto-fill, ${String(coverWidth)}px)` }}
      className="grid content-start justify-between gap-x-6 gap-y-7 outline-none"
    >
      {(book) => (
        <GridListItem
          id={book.id}
          textValue={book.title}
          className="group/cover min-w-0 rounded-md outline-none data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-(--focus-ring)"
        >
          <BookCard book={book} />
        </GridListItem>
      )}
    </GridList>
  );
}

const READING_LABELS = {
  want_to_read: "Want to read",
  reading: "Reading",
  on_hold: "On hold",
  finished: "Finished",
  abandoned: "Abandoned",
};

function BookCard({ book }: Readonly<{ book: BookListItem }>): ReactElement {
  const [coverFailed, setCoverFailed] = useState(false);
  const reading = book.reading_state;
  const progress = reading?.progress_pct ?? 0;
  let label =
    reading?.status === null || reading === null ? "None" : READING_LABELS[reading.status];
  if (reading?.status === "reading" && reading.progress_pct !== null)
    label = `${String(Math.round(progress))}% read`;
  let statusIcon = <Circle aria-hidden="true" className="size-3" />;
  if (reading?.status === "finished")
    statusIcon = <CircleCheck aria-hidden="true" className="size-3" />;
  return (
    <div className="flex flex-col gap-3">
      <div className="relative aspect-[2/3] rounded-md border border-hairline bg-surface shadow-[0_8px_16px_color-mix(in_oklab,var(--fg)_15%,transparent)] dark:shadow-none group-data-selected/cover:outline-2 group-data-selected/cover:outline-offset-2 group-data-selected/cover:outline-accent">
        <div className="size-full overflow-hidden rounded-[inherit]">
          {book.cover_url === "" || coverFailed ? (
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
          {reading?.status === "reading" && progress > 0 ? (
            <Progress
              aria-label={`Reading progress for ${book.title}`}
              value={progress}
              className="absolute inset-x-0 bottom-0 h-1 rounded-b-md"
            />
          ) : null}
        </div>
        <Checkbox
          slot="selection"
          aria-label={`Select ${book.title}`}
          className="absolute left-2 top-2 opacity-0 group-data-hovered/cover:opacity-100 group-data-selected/cover:opacity-100 group-focus-within/cover:opacity-100"
        />
      </div>
      <div className="flex min-w-0 flex-col gap-1 text-xs">
        <h3 className="truncate text-[13px] font-semibold text-fg">
          <Link href={`/b/${book.id}`} className="block truncate text-[13px] font-semibold text-fg">
            {book.title}
          </Link>
        </h3>
        <p className="truncate text-fg-muted">{book.authors.join(", ")}</p>
        <div className="mt-1 flex items-center justify-between gap-1 text-fg-muted">
          <span
            className={`flex min-w-0 items-center gap-1.5 ${reading?.status === "reading" ? "font-semibold text-accent-text" : ""}`}
          >
            {reading?.status === "reading" ? null : statusIcon}
            <span className="truncate">{label}</span>
          </span>
          {reading?.rating === null || reading === null ? null : (
            <span
              aria-label={`${String(reading.rating)} out of 5 stars`}
              className="flex shrink-0 gap-0.5 text-fg"
            >
              {Array.from({ length: reading.rating }, (_, index) => (
                <Star
                  key={String(index + 1)}
                  aria-hidden="true"
                  className="size-2.5 fill-current"
                />
              ))}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

type FilteredEmptyStateProps = {
  /** Drops every filter param, returning the browse to the full library. */
  onClear: () => void;
};

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
      <Skeleton className="mb-8 h-6 w-24" />
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
