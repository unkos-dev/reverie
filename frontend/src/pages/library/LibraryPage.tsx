import {
  graphql,
  useFragment,
  usePaginationFragment,
  usePreloadedQuery,
  useRefetchableFragment,
  useRelayEnvironment,
} from "react-relay";
import { GraphQLExecutionError, editingFailureAction } from "@/api/graphql";
import { ErrorBoundary } from "react-error-boundary";
import {
  bindLibraryVisit,
  invalidateLibraryVisits,
  libraryRequest,
  libraryLookups,
  type LibrarySource,
  type LibraryVisitOwner,
} from "@/lib/relay/library-visit";
import { readerLibrary, readerShelves } from "@/lib/relay/reader";
import type { LibraryPageQuery as PageQuery } from "@/lib/relay/__generated__/LibraryPageQuery.graphql";
import type { LibraryPage_labels$key } from "@/lib/relay/__generated__/LibraryPage_labels.graphql";
import type { LibraryPageLabelsRefetchQuery as LabelsRefetch } from "@/lib/relay/__generated__/LibraryPageLabelsRefetchQuery.graphql";
import type { readerShelves$key } from "@/lib/relay/__generated__/readerShelves.graphql";
import type { readerShelvesPaginationQuery as ShelvesPagination } from "@/lib/relay/__generated__/readerShelvesPaginationQuery.graphql";
import type {
  readerLibrary$data,
  readerLibrary$key,
} from "@/lib/relay/__generated__/readerLibrary.graphql";
import type { readerLibraryPaginationQuery as BooksPagination } from "@/lib/relay/__generated__/readerLibraryPaginationQuery.graphql";
import type { BookCard_book$key } from "@/lib/relay/__generated__/BookCard_book.graphql";
import { commitMutation } from "relay-runtime";
import { useAuthMe } from "@/hooks/useAuthMe";
import { mutationsAddShelfItemMutation } from "@/lib/relay/mutations";
import type { mutationsAddShelfItemMutation as AddShelfItem } from "@/lib/relay/__generated__/mutationsAddShelfItemMutation.graphql";
import type { LibraryPageRefreshMetadataMutation as RefreshMetadata } from "@/lib/relay/__generated__/LibraryPageRefreshMetadataMutation.graphql";
import { Circle, CircleCheck, Loader2, Star } from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useEffectEvent,
  useSyncExternalStore,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type ReactElement,
} from "react";
import { useLoaderData, useNavigate } from "react-router";

import { type SortLevelParam } from "@/api";
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
import { useLibraryFilters, type LibraryFilters } from "@/lib/hooks/use-library-filters";
import type { loader } from "@/routes/library";

import {
  countActiveFilters,
  filterStateToParams,
  hasActiveFilterState,
  hasOverLimitSelection,
  type LibraryView,
  type LibraryPosition,
  type LibraryVisit,
} from "@/routes/library-params";

import { useCellEdit } from "./table/useCellEdit";
import { GridShortcutsDialog, GridShortcutsTrigger } from "./table/GridShortcutsDialog";
import { BatchBar, type BatchAction, type BatchOutcome } from "./BatchBar";
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

const LibraryPageQuery = graphql`
  query LibraryPageQuery(
    $authorIds: [ID!]!
    $seriesIds: [ID!]!
    $shelfIds: [ID!]!
    $eligible: Boolean!
    $sourceKey: String!
    $filter: BooksFilter
    $sort: [SortLevel!]
    $count: Int = 50
  ) @throwOnFieldError {
    viewer {
      id
      libraryPreferences {
        ...useLibraryPreferences_preferences
      }
    }
    ...LibraryPage_labels
      @arguments(authorIds: $authorIds, seriesIds: $seriesIds, shelfIds: $shelfIds)
    ...readerShelves
    ...TypeaheadMultiSelect_suggestions
    ...readerLibrary
      @alias(as: "rows")
      @include(if: $eligible)
      @arguments(count: $count, sourceKey: $sourceKey, filter: $filter, sort: $sort)
  }
`;

const refreshMetadataMutation = graphql`
  mutation LibraryPageRefreshMetadataMutation($input: RefreshMetadataInput!) {
    refreshMetadata(input: $input) {
      __typename
      ... on MetadataRefreshQueued {
        manifestationId
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const labelsFragment = graphql`
  fragment LibraryPage_labels on Query
  @refetchable(queryName: "LibraryPageLabelsRefetchQuery")
  @argumentDefinitions(
    authorIds: { type: "[ID!]!" }
    seriesIds: { type: "[ID!]!" }
    shelfIds: { type: "[ID!]!" }
  )
  @throwOnFieldError {
    authorLabels(ids: $authorIds) {
      id
      name
    }
    seriesLabels(ids: $seriesIds) {
      id
      name
    }
    shelfLabels(ids: $shelfIds) {
      id
      name
      itemCount
    }
  }
`;

const COVER_WIDTH_KEY = "reverie:cover-width";
const CoverWidthSchema = z.coerce.number().int().min(120).max(300);

function saveCoverWidth(width: number): void {
  try {
    localStorage.setItem(COVER_WIDTH_KEY, String(width));
  } catch (error) {
    console.error("[LibraryPage] cover width could not be saved", error);
  }
}

type LibraryRow = readerLibrary$data["books"]["edges"][number]["node"];

function useLibraryBooks(
  visit: LibraryVisitOwner,
  initial: LibrarySource,
  owner: LibraryFilters,
  sortOverride: string,
  sort: readonly SortLevelParam[],
  intent: string,
) {
  const state = useSyncExternalStore(visit.subscribe, visit.snapshot);
  const params = filterStateToParams(owner.filters);
  const request = libraryRequest(owner.filters, sortOverride);
  const savedPosition = useEffectEvent(() => owner.position);
  useLayoutEffect(() => {
    bindLibraryVisit(visit);
    visit.enter(initial, savedPosition());
    return () => {
      visit.leave();
    };
  }, [visit, initial]);
  useLayoutEffect(() => {
    visit.activate(request, sort, intent);
  }, [visit, request, sort, intent]);
  const query = usePreloadedQuery<PageQuery>(LibraryPageQuery, (state.source ?? initial).reference);
  const { data, hasNext, isLoadingNext, loadNext } = usePaginationFragment<
    BooksPagination,
    readerLibrary$key
  >(readerLibrary, state.source !== undefined ? (query.rows ?? null) : null);
  const [pageError, setPageError] = useState<{ source: LibrarySource; error: Error }>();
  const moreError = pageError?.source === state.source && pageError !== undefined;
  const rejected =
    state.error instanceof GraphQLExecutionError &&
    state.error.errors.some((error) => error.extensions?.code === "validation");
  const blocked = !request.eligible || rejected;
  let blockedMessage: string | undefined;
  if (!request.eligible)
    blockedMessage = hasOverLimitSelection(params)
      ? "Results are paused. Choose 20 or fewer values in each filter mode, or clear the filters."
      : "Results are paused. Shorten the search or filter text to 200 characters or fewer, or clear the filters.";
  else if (rejected)
    blockedMessage = "These filters could not be used. Change or clear them to try again.";
  if (
    state.error !== null &&
    !blocked &&
    (state.source === undefined || JSON.stringify(state.source.request) !== JSON.stringify(request))
  )
    throw state.error;
  return {
    ...state,
    blockedMessage,
    items: data?.books.edges.map((edge) => edge.node) ?? [],
    updating: !blocked && state.candidate !== undefined,
    canLoadMore: !blocked && state.candidate === undefined && hasNext,
    loading: request.eligible && state.source === undefined && state.error === null,
    showContradiction: !blocked && state.candidate === undefined && state.error === null,
    fetchNextPage: () => {
      const source = state.source;
      if (source === undefined || !hasNext || isLoadingNext || state.candidate !== undefined)
        return;
      setPageError(undefined);
      loadNext(50, {
        onComplete: (error) => {
          if (error !== null) setPageError({ source, error });
        },
      });
    },
    isFetchingNextPage: isLoadingNext,
    isFetchNextPageError: moreError,
  };
}

function LibraryCandidate({
  source,
  owner,
}: Readonly<{ source: LibrarySource; owner: LibraryVisitOwner }>): null {
  const query = usePreloadedQuery<PageQuery>(LibraryPageQuery, source.reference);
  const { data, hasNext, isLoadingNext, loadNext } = usePaginationFragment<
    BooksPagination,
    readerLibrary$key
  >(readerLibrary, query.rows ?? null);
  useEffect(() => {
    if (data == null || isLoadingNext) return;
    const ids = data.books.edges.map((edge) => edge.node.id);
    if (owner.pageCommitted(source, ids, hasNext))
      loadNext(50, {
        onComplete: (error) => {
          if (error !== null) owner.failed(source, error);
        },
      });
  }, [data, hasNext, isLoadingNext, loadNext, owner, source]);
  return null;
}

function useLibraryPosition(
  visit: LibraryVisitOwner,
  owner: LibraryFilters,
  source: LibrarySource | undefined,
  items: readonly LibraryRow[],
  view: LibraryView,
  density: "comfortable" | "compact",
  position: LibraryPosition | undefined,
  generation: number,
) {
  const table = useRef<HTMLTableElement | null>(null);
  const covers = useRef<HTMLDivElement | null>(null);
  const expected = useRef<{ element: Element; offset: number } | undefined>(undefined);
  const frame = useRef<number | undefined>(undefined);

  const scrollElement = useCallback(() => {
    if (view === "table") return table.current;
    const element = covers.current;
    return element !== null && element.scrollHeight > element.clientHeight + 1
      ? element
      : document.scrollingElement;
  }, [view]);
  const capture = useCallback((): LibraryPosition | undefined => {
    const element = scrollElement();
    if (element === null || source === undefined) return undefined;
    const documentScroll = element === document.scrollingElement;
    const rect = element.getBoundingClientRect();
    let start: number;
    if (documentScroll)
      start = document.querySelector("header[data-chrome]")?.getBoundingClientRect().bottom ?? 0;
    else start = rect.top + (view === "table" ? 42 : 0);
    const end = documentScroll ? window.innerHeight : rect.bottom;
    const anchor = [...element.querySelectorAll<HTMLElement>("[data-book-id]")]
      .map((row) => ({ row, rect: row.getBoundingClientRect() }))
      .filter(({ rect }) => rect.bottom > start && rect.top < end)
      .sort((a, b) => a.rect.top - b.rect.top || a.rect.left - b.rect.left)
      .at(0);
    return {
      anchorId: anchor?.row.dataset.bookId ?? null,
      anchorOffset: (anchor?.rect.top ?? start) - start,
      offset: element.scrollTop,
      extent: items.length,
      view,
    };
  }, [source, view, scrollElement, items.length]);
  const scrolled = useCallback(() => {
    const element = scrollElement();
    if (element === null || position === undefined) return;
    if (
      expected.current?.element === element &&
      Math.abs(
        element.scrollTop -
          Math.min(
            expected.current.offset,
            Math.max(0, element.scrollHeight - element.clientHeight),
          ),
      ) < 1
    )
      return;
    if (frame.current !== undefined) cancelAnimationFrame(frame.current);
    expected.current = undefined;
    visit.cancelRestoration();
  }, [position, visit, scrollElement]);
  const anchorIndex =
    position?.anchorId == null ? -1 : items.findIndex((row) => row.id === position.anchorId);
  const restore = useCallback(() => {
    if (
      position === undefined ||
      position.view !== view ||
      source === undefined ||
      navigation.currentEntry?.id !== visit.entryId
    )
      return;
    let element: Element | null = table.current;
    if (view === "grid")
      element =
        covers.current !== null && covers.current.scrollHeight > covers.current.clientHeight + 1
          ? covers.current
          : document.scrollingElement;
    if (element === null) return;
    const index = anchorIndex;
    function setOffset(offset: number): void {
      if (element === null) return;
      element.scrollTo({ top: offset, behavior: "instant" });
      expected.current = { element, offset: element.scrollTop };
    }
    if (view === "table" && index >= 0)
      setOffset(index * (density === "compact" ? 40 : 52) - position.anchorOffset);
    const adjust = (): void => {
      if (
        visit.snapshot().source !== source ||
        visit.snapshot().restoreGeneration !== generation ||
        navigation.currentEntry?.id !== visit.entryId
      )
        return;
      if (index < 0) {
        setOffset(
          position.offset > element.scrollHeight - element.clientHeight ? 0 : position.offset,
        );
        return;
      }
      if (view === "table")
        setOffset(index * (density === "compact" ? 40 : 52) - position.anchorOffset);
      const anchor = [...element.querySelectorAll<HTMLElement>("[data-book-id]")].find(
        (row) => row.dataset.bookId === position.anchorId,
      );
      if (anchor === undefined) return;
      let start: number;
      if (element === document.scrollingElement)
        start = document.querySelector("header[data-chrome]")?.getBoundingClientRect().bottom ?? 0;
      else start = element.getBoundingClientRect().top + (view === "table" ? 42 : 0);
      setOffset(
        element.scrollTop + anchor.getBoundingClientRect().top - start - position.anchorOffset,
      );
    };
    if (view === "grid" || index < 0) adjust();
    if (frame.current !== undefined) cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(adjust);
  }, [position, view, source, visit, density, generation, anchorIndex]);

  useLayoutEffect(() => {
    restore();
    return () => {
      if (frame.current !== undefined) cancelAnimationFrame(frame.current);
    };
  }, [restore]);
  useEffect(() => {
    const unregister = visit.registerCapture(capture);
    const leaving = (): void => {
      if (navigation.currentEntry?.id !== visit.entryId) return;
      const next = capture();
      if (next !== undefined) {
        visit.capture(next);
        owner.capturePosition(next);
      }
    };
    navigation.addEventListener("navigate", leaving);
    window.addEventListener("scroll", scrolled);
    return () => {
      unregister();
      navigation.removeEventListener("navigate", leaving);
      window.removeEventListener("scroll", scrolled);
    };
  }, [visit, owner, capture, scrolled]);
  const tableRef = useCallback(
    (element: HTMLTableElement | null) => {
      table.current = element;
      restore();
    },
    [restore],
  );
  const coversRef = useCallback(
    (element: HTMLDivElement | null) => {
      covers.current = element;
      restore();
    },
    [restore],
  );
  return { tableRef, coversRef, scrolled };
}

function TableEditControls(): ReactElement {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  return (
    <>
      <GridShortcutsTrigger onOpenChange={setShortcutsOpen} />
      <GridShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
    </>
  );
}

type BatchAttempt = { action: BatchAction; ids: readonly string[] };
type BatchSendResult = { saved: boolean } & Omit<BatchOutcome["failed"][number], "id">;

export function LibraryPage(): ReactElement | null {
  const prepared = useLoaderData<typeof loader>();
  useLayoutEffect(() => {
    if (prepared === null) return;
    return prepared.preparation.attach(() => {
      prepared.owner.discard(prepared.preparation);
    });
  }, [prepared]);
  if (prepared === null) return null;
  return (
    <Suspense fallback={<LibrarySkeleton />}>
      <LibraryContent prepared={prepared} />
    </Suspense>
  );
}

function sameIds(left: readonly { id: string }[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((item, index) => item.id === right[index]);
}

function useLibraryLabels(query: LibraryPage_labels$key) {
  const [labels, refetch] = useRefetchableFragment<LabelsRefetch, LibraryPage_labels$key>(
    labelsFragment,
    query,
  );
  function follow(next: LibraryVisit["filters"], write: () => void): void {
    const lookups = libraryLookups(next);
    write();
    if (
      sameIds(labels.authorLabels, lookups.authorIds) &&
      sameIds(labels.seriesLabels, lookups.seriesIds) &&
      sameIds(labels.shelfLabels, lookups.shelfIds)
    )
      return;
    refetch(lookups, { fetchPolicy: "store-or-network" });
  }
  return { labels, follow };
}

function LibraryContent({
  prepared,
}: Readonly<{ prepared: NonNullable<Awaited<ReturnType<typeof loader>>> }>): ReactElement {
  const { owner: visitOwner, preparation, visit: restored, initial } = prepared;
  useLayoutEffect(() => {
    preparation.adopt();
  }, [preparation]);
  const query = usePreloadedQuery<PageQuery>(LibraryPageQuery, initial.reference);
  const { labels, follow } = useLibraryLabels(query);
  const owner = useLibraryFilters(visitOwner.intent, restored, follow);
  const { filters: filterState, commitAll, clearAll, setView: setVisitView } = owner;
  const authorNames = new Map(labels.authorLabels.map((author) => [author.id, author.name]));
  const seriesNames = new Map(labels.seriesLabels.map((series) => [series.id, series.name]));
  const shelfNames = new Map(labels.shelfLabels.map((shelf) => [shelf.id, shelf.name]));
  const labelFor = {
    authorLabel: (id: string) => authorNames.get(id) ?? "",
    seriesName: (id: string) => seriesNames.get(id) ?? "",
    shelfName: (id: string) => shelfNames.get(id) ?? "",
  };
  const {
    data: shelfData,
    hasNext: moreShelves,
    isLoadingNext: loadingShelves,
    loadNext: loadShelves,
  } = usePaginationFragment<ShelvesPagination, readerShelves$key>(readerShelves, query);
  const [shelvesFailure, setShelvesFailure] = useState<Error | null>(null);
  useEffect(() => {
    if (shelvesFailure === null && moreShelves && !loadingShelves)
      loadShelves(50, {
        onComplete: (failure) => {
          if (failure === null) return;
          console.error("[LibraryPage] shelf page failed", failure);
          setShelvesFailure(failure);
        },
      });
  }, [shelvesFailure, moreShelves, loadingShelves, loadShelves]);
  const shelves = shelfData.shelves.edges.map((edge) => edge.node);
  const shelvesError = shelvesFailure !== null;
  const railShelves = {
    items: shelves,
    loading: !shelvesError && (moreShelves || loadingShelves),
    failed: shelvesError,
  };
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
  const environment = useRelayEnvironment();
  const { data: account } = useAuthMe();
  const batchController = useRef<AbortController | null>(null);
  useEffect(() => {
    const lifetime = new AbortController();
    batchController.current = lifetime;
    return () => {
      lifetime.abort();
    };
  }, []);
  const [batch, setBatch] = useState<BatchOutcome>();

  const [hasRenderedControls, setHasRenderedControls] = useState(false);
  const preferences = useLibraryPreferences(query.viewer.id, query.viewer.libraryPreferences);
  const { density, hiddenColumns, sortOverride, sortLevels, defaultSortLevels, setSortLevels } =
    preferences;

  const viewMode: LibraryView = owner.view ?? preferences.view;
  const queryToken = JSON.stringify([filterState, sortOverride, viewMode]);
  const liveIntent = useRef(queryToken);
  useLayoutEffect(() => {
    liveIntent.current = queryToken;
  }, [queryToken]);

  const {
    source,
    candidate,
    candidateReady,
    items,
    position,
    restoreGeneration,
    blockedMessage,
    updating,
    canLoadMore,
    loading,
    showContradiction,
    fetchNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  } = useLibraryBooks(visitOwner, initial, owner, sortOverride, sortLevels, queryToken);
  const geometry = useLibraryPosition(
    visitOwner,
    owner,
    source,
    items,
    viewMode,
    density,
    position,
    restoreGeneration,
  );
  const edits = useCellEdit({ enabled: viewMode === "table", rows: items });
  const selectionToken = JSON.stringify([queryToken, source?.key]);
  const [syncedSelectionToken, setSyncedSelectionToken] = useState(selectionToken);
  if (selectionToken !== syncedSelectionToken) {
    setSyncedSelectionToken(selectionToken);
    if (selectedIds.size > 0) setSelectedIds(new Set());
  }

  function sendBatchShelf(id: string, shelfId: string): Promise<BatchSendResult> {
    return new Promise((resolve, reject) => {
      commitMutation<AddShelfItem>(environment, {
        mutation: mutationsAddShelfItemMutation,
        variables: {
          input: { shelfId, manifestationId: id },
          connections: [],
        },
        updater: (store, data) => {
          if (data?.addShelfItem.__typename === "ShelfItemAdded")
            store.get(shelfId)?.invalidateRecord();
        },
        onCompleted: ({ addShelfItem }) => {
          if (addShelfItem.__typename === "ShelfItemAdded") {
            invalidateLibraryVisits(environment, [], { shelfId });
            resolve({ saved: true, action: "none" });
          } else if (addShelfItem.__typename === "NotFound")
            resolve({ saved: false, action: "none", message: addShelfItem.message });
          else reject(new Error("Unexpected shelf result."));
        },
        onError: reject,
      });
    });
  }

  function sendBatchRefresh(id: string): Promise<BatchSendResult> {
    return new Promise((resolve, reject) => {
      commitMutation<RefreshMetadata>(environment, {
        mutation: refreshMetadataMutation,
        variables: { input: { id } },
        onCompleted: ({ refreshMetadata }) => {
          if (refreshMetadata.__typename === "MetadataRefreshQueued")
            resolve({ saved: true, action: "none" });
          else
            resolve({
              saved: false,
              action: "none",
              message:
                refreshMetadata.__typename === "NotFound"
                  ? refreshMetadata.message
                  : "Request failed.",
            });
        },
        onError: reject,
      });
    });
  }

  async function sendBatchItem(
    attempt: BatchAttempt,
    id: string,
    statusTargets: ReturnType<typeof edits.statusTargets>,
  ): Promise<BatchSendResult | undefined> {
    if (attempt.action.kind === "status") {
      const target = statusTargets.find((item) => item.id === id);
      const saved = target !== undefined && (await edits.saveStatus(target, attempt.action.status));
      return saved === undefined ? undefined : { saved, action: "none" };
    }
    if (attempt.action.kind === "shelf") return sendBatchShelf(id, attempt.action.shelf.id);
    return sendBatchRefresh(id);
  }

  async function sendBatchItems(
    attempt: BatchAttempt,
    succeeded: string[],
    failed: BatchOutcome["failed"][number][],
    statusTargets: ReturnType<typeof edits.statusTargets>,
  ): Promise<void> {
    for (const id of attempt.ids) {
      if (batchController.current?.signal.aborted) break;
      try {
        const result = await sendBatchItem(attempt, id, statusTargets);
        if (result === undefined) break;
        if (result.saved) succeeded.push(id);
        else failed.push({ id, action: result.action, message: result.message });
      } catch (error) {
        console.error("[LibraryPage] batch action failed", { id, error });
        if (attempt.action.kind === "status") {
          failed.push({ id, action: "none" });
        } else {
          failed.push({
            id,
            action: editingFailureAction(error),
            message: error instanceof Error ? error.message : "Request failed.",
          });
        }
      }
    }
  }

  async function runBatch(attempt: BatchAttempt, retrying = false): Promise<void> {
    if (batch?.pending === true) return;
    const previous = retrying ? batch : undefined;
    const succeeded: string[] = [];
    const failed: BatchOutcome["failed"][number][] =
      previous?.failed.filter((failure) => !attempt.ids.includes(failure.id)) ?? [];
    const total = previous?.total ?? attempt.ids.length;
    const statusTargets =
      attempt.action.kind === "status"
        ? edits.statusTargets(attempt.ids.map((id) => ({ id })))
        : [];
    setBatch({
      action: attempt.action,
      pending: true,
      retrying,
      total,
      succeeded: previous?.succeeded ?? [],
      failed: [...failed],
    });
    await sendBatchItems(attempt, succeeded, failed, statusTargets);
    if (batchController.current?.signal.aborted) return;
    setBatch({
      action: attempt.action,
      pending: false,
      retrying,
      total,
      succeeded: [...(previous?.succeeded ?? []), ...succeeded],
      failed,
    });
    if (liveIntent.current === queryToken) {
      setSelectedIds(
        (current) =>
          new Set([
            ...[...current].filter((id) => !succeeded.includes(id)),
            ...failed
              .filter((failure) => items.some((row) => row.id === failure.id))
              .map((failure) => failure.id),
          ]),
      );
    }
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
    preferences.setColumnHidden(key, hidden);
  }

  const activeFilterCount = countActiveFilters(filterState);

  if (!loading && !hasRenderedControls) setHasRenderedControls(true);
  const candidateContent =
    candidate === undefined || !candidateReady ? null : (
      <ErrorBoundary
        key={candidate.key}
        fallback={null}
        onError={(error) => {
          visitOwner.failed(
            candidate,
            error instanceof Error ? error : new Error("Library request failed."),
          );
        }}
      >
        <Suspense fallback={null}>
          <LibraryCandidate owner={visitOwner} source={candidate} />
        </Suspense>
      </ErrorBoundary>
    );
  if (loading && !hasRenderedControls)
    return (
      <>
        {candidateContent}
        <LibrarySkeleton />
      </>
    );

  return (
    <>
      {candidateContent}
      <div className="lg:h-[calc(100dvh-86px)]">
        <div className="flex h-full min-h-0 flex-col px-3 pb-3 sm:px-5">
          <ShelfHeader shelves={labels.shelfLabels} selected={filterState.shelf} />
          {/* Sort announcements stay outside the filter editor so hidden controls do not silence them. */}
          <p className="sr-only" aria-live="polite">
            {sortStackSummary(sortLevels)}
          </p>
          <LibraryToolbar
            filters={
              <FilterChips
                filters={filterState}
                labels={labelFor}
                onRemove={commitAll}
                onClearAll={clearAll}
              />
            }
            editControls={viewMode === "table" ? <TableEditControls /> : null}
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
              {viewMode === "table" && edits.error !== null ? (
                <div className="border-border bg-surface mb-3 flex items-center gap-3 rounded-md border px-3 py-2">
                  <p role="alert" className="text-danger-text min-w-0 flex-1 break-words text-sm">
                    {edits.error}
                  </p>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onPress={edits.dismissError}
                    aria-label="Dismiss edit error"
                  >
                    Dismiss
                  </Button>
                </div>
              ) : null}
              <LibraryBooks
                scrollRef={geometry.coversRef}
                onScroll={geometry.scrolled}
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
                      scrollRef={geometry.tableRef}
                      onScroll={geometry.scrolled}
                      items={items}
                      sort={sortLevels}
                      defaultSort={defaultSortLevels}
                      onSortChange={handleSortChange}
                      hasNextPage={canLoadMore}
                      isFetchingNextPage={isFetchingNextPage}
                      isFetchNextPageError={isFetchNextPageError}
                      onLoadMore={() => {
                        fetchNextPage();
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
                    fetchNextPage();
                  }}
                />
              ) : null}
            </div>
            {filtersBeside ? (
              <div className="library-filter-slot" inert={!filtersOpen}>
                <FilterRail
                  owner={owner}
                  suggestions={query}
                  labels={labelFor}
                  shelves={railShelves}
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
            canRefresh={account !== undefined && !account.is_child}
            result={batch}
            onAction={(action) => {
              const ids = items.filter((row) => selectedIds.has(row.id)).map((row) => row.id);
              void runBatch({ action, ids });
            }}
            onRetry={() => {
              if (batch === undefined || batch.action.kind === "status") return;
              const ids = batch.failed
                .filter((failure) => failure.action === "retry")
                .map((failure) => failure.id);
              void runBatch({ action: batch.action, ids }, true);
            }}
            onDismiss={() => {
              setBatch(undefined);
            }}
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
            suggestions={query}
            labels={labelFor}
            shelves={railShelves}
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
  scrollRef: (element: HTMLDivElement | null) => void;
  onScroll: () => void;
  items: LibraryRow[];
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
  scrollRef,
  onScroll,
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
        ref={scrollRef}
        onScroll={onScroll}
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
  items: LibraryRow[];
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
          data-book-id={book.id}
          textValue={book.metadata.title}
          className="group/cover min-w-0 rounded-md outline-none data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-(--focus-ring)"
        >
          <BookCard book={book} />
        </GridListItem>
      )}
    </GridList>
  );
}

const READING_LABELS: Record<string, string> = {
  WANT_TO_READ: "Want to read",
  READING: "Reading",
  ON_HOLD: "On hold",
  FINISHED: "Finished",
  ABANDONED: "Abandoned",
};

const bookCardFragment = graphql`
  fragment BookCard_book on Manifestation @throwOnFieldError {
    id
    coverUrl
    metadata {
      title
    }
    work {
      title
      authors(first: 5) {
        edges {
          node {
            name
          }
        }
      }
    }
    readingState {
      status
      progressPct
      rating
    }
  }
`;

function BookCard({ book: fragment }: Readonly<{ book: BookCard_book$key }>): ReactElement {
  const book = useFragment(bookCardFragment, fragment);
  const title = book.work?.title ?? book.metadata.title;
  const authors = book.work?.authors.edges.map((edge) => edge.node.name) ?? [];
  const [coverFailed, setCoverFailed] = useState(false);
  const reading = book.readingState;
  const progress = reading.progressPct ?? 0;
  let label = reading.status == null ? "None" : (READING_LABELS[reading.status] ?? "None");
  if (reading.status === "READING" && reading.progressPct != null)
    label = `${String(Math.round(progress))}% read`;
  let statusIcon = <Circle aria-hidden="true" className="size-3" />;
  if (reading.status === "FINISHED")
    statusIcon = <CircleCheck aria-hidden="true" className="size-3" />;
  return (
    <div className="flex flex-col gap-3">
      <div className="relative aspect-[2/3] rounded-md border border-hairline bg-surface shadow-[0_8px_16px_color-mix(in_oklab,var(--fg)_15%,transparent)] dark:shadow-none group-data-selected/cover:outline-2 group-data-selected/cover:outline-offset-2 group-data-selected/cover:outline-accent">
        <div className="size-full overflow-hidden rounded-[inherit]">
          {book.coverUrl === "" || coverFailed ? (
            <CoverArtwork bookId={book.id} title={title} authors={authors} />
          ) : (
            <img
              src={book.coverUrl}
              alt={`Cover of ${title}`}
              loading="lazy"
              decoding="async"
              onError={() => {
                setCoverFailed(true);
              }}
              className="size-full object-cover"
            />
          )}
          {reading.status === "READING" && progress > 0 ? (
            <Progress
              aria-label={`Reading progress for ${title}`}
              value={progress}
              className="absolute inset-x-0 bottom-0 h-1 rounded-b-md"
            />
          ) : null}
        </div>
        <Checkbox
          slot="selection"
          aria-label={`Select ${title}`}
          className="absolute left-2 top-2 opacity-0 group-data-hovered/cover:opacity-100 group-data-selected/cover:opacity-100 group-focus-within/cover:opacity-100"
        />
      </div>
      <div className="flex min-w-0 flex-col gap-1 text-xs">
        <h3 className="truncate text-[13px] font-semibold text-fg">
          <Link href={`/b/${book.id}`} className="block truncate text-[13px] font-semibold text-fg">
            {title}
          </Link>
        </h3>
        <p className="truncate text-fg-muted">{authors.join(", ")}</p>
        <div className="mt-1 flex items-center justify-between gap-1 text-fg-muted">
          <span
            className={`flex min-w-0 items-center gap-1.5 ${reading.status === "READING" ? "font-semibold text-accent-text" : ""}`}
          >
            {reading.status === "READING" ? null : statusIcon}
            <span className="truncate">{label}</span>
          </span>
          {reading.rating == null ? null : (
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
