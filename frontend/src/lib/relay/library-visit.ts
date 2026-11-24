import { fetchQuery, loadQuery, type PreloadedQuery } from "react-relay";
import {
  ConnectionHandler,
  ROOT_ID,
  createOperationDescriptor,
  type Environment,
  type ReadOnlyRecordProxy,
  type Subscription,
} from "relay-runtime";

import { GraphQLExecutionError } from "@/api/graphql";
import {
  parseSortParam,
  type ListBooksParams,
  type SortField,
  type SortLevelParam,
} from "@/api/books";
import { LibraryIntent } from "@/lib/hooks/use-library-filters";
import {
  emptyFilterState,
  filterStateToParams,
  isLibraryRequestEligible,
  type FilterState,
  type LibraryPosition,
  type LibraryVisit,
} from "@/routes/library-params";
import LibraryPageDocument, {
  type LibraryPageQuery as LibraryPageQuery,
  type SortField as GraphQLSortField,
} from "./__generated__/LibraryPageQuery.graphql";

const SORT_COLUMNS = {
  title: "TITLE",
  author: "AUTHOR",
  created_at: "CREATED_AT",
  pages: "PAGES",
} as const satisfies Record<SortField, GraphQLSortField>;

function libraryVariables(
  params: ListBooksParams,
): Pick<LibraryPageQuery["variables"], "count" | "filter" | "sort"> {
  const filter = {
    q: params.q,
    author: params.author,
    authorAny: params.author_any,
    authorNone: params.author_none,
    series: params.series,
    seriesAny: params.series_any,
    shelf: params.shelf,
    shelfAny: params.shelf_any,
    tag: params.tag,
    tagAny: params.tag_any,
    tagNone: params.tag_none,
    genre: params.genre,
    genreAny: params.genre_any,
    genreNone: params.genre_none,
    mood: params.mood,
    moodAny: params.mood_any,
    moodNone: params.mood_none,
    statusAny: params.status_any,
    statusNone: params.status_none,
    titleContains: params.title_contains,
    titleEq: params.title_eq,
    titleNe: params.title_ne,
    subtitleContains: params.subtitle_contains,
    subtitleEmpty: params.subtitle_empty,
    isbn13Contains: params.isbn_13_contains,
    isbn13Eq: params.isbn_13_eq,
    isbn13Empty: params.isbn_13_empty,
    pagesGte: params.pages_gte,
    pagesLte: params.pages_lte,
    pagesEmpty: params.pages_empty,
    ratingGte: params.rating_gte,
    ratingLte: params.rating_lte,
    ratingEmpty: params.rating_empty,
    createdAtGte: params.created_at_gte,
    createdAtLt: params.created_at_lt,
  };
  return {
    count: 50,
    filter,
    ...(params.sort === undefined || params.sort === ""
      ? {}
      : {
          sort: parseSortParam(params.sort).map((level) => ({
            column: SORT_COLUMNS[level.field],
            direction: level.desc ? "DESC" : "ASC",
          })),
        }),
  };
}

type LibraryRequest = Omit<LibraryPageQuery["variables"], "sourceKey">;

export function libraryRequest(filters: FilterState, sort: string): LibraryRequest {
  const params = filterStateToParams(filters);
  if (sort !== "") params.sort = sort;
  const { filter, sort: levels, count } = libraryVariables(params);
  return {
    ...libraryLookups(filters),
    eligible: isLibraryRequestEligible(params),
    filter,
    sort: levels,
    count,
  };
}

export class LibrarySource {
  readonly operation;
  readonly request: LibraryRequest;
  readonly key: string;
  readonly sort: readonly SortLevelParam[];
  readonly variables: LibraryPageQuery["variables"];
  readonly reference: PreloadedQuery<LibraryPageQuery>;

  constructor(
    environment: Environment,
    request: LibraryRequest,
    key: string,
    sort: readonly SortLevelParam[],
    storeOnly = false,
  ) {
    this.request = request;
    this.key = key;
    this.sort = sort;
    this.variables = { ...request, sourceKey: key };
    this.operation = createOperationDescriptor(LibraryPageDocument, this.variables);
    this.reference = loadQuery<LibraryPageQuery>(environment, LibraryPageDocument, this.variables, {
      fetchPolicy: storeOnly ? "store-only" : "store-or-network",
    });
  }

  dispose(): void {
    this.reference.dispose();
  }
}

type VisitState = {
  source: LibrarySource | undefined;
  candidate: LibrarySource | undefined;
  candidateReady: boolean;
  error: Error | null;
  restoreGeneration: number;
  position: LibraryPosition | undefined;
};

type Disposable = { dispose: () => void };

/** The query references one route load created, owned from the moment each exists. */
export class LibraryPreparation {
  private readonly held: Disposable[] = [];
  private dropped = false;
  private adopted = false;
  private pages = 0;
  private unwatch: (() => void) | undefined;

  hold<T extends Disposable>(reference: T): T {
    if (this.dropped) reference.dispose();
    else this.held.push(reference);
    return reference;
  }

  /** Abandon the preparation if its navigation aborts before a page takes it. */
  watch(signal: AbortSignal, abandon: () => void): void {
    if (signal.aborted) {
      abandon();
      return;
    }
    signal.addEventListener("abort", abandon, { once: true });
    this.unwatch = () => {
      signal.removeEventListener("abort", abandon);
    };
  }

  /**
   * Hand the preparation to a mounted page. It is abandoned once every page has left before its
   * content rendered, which a failed screen query or a superseding navigation causes.
   */
  attach(abandon: () => void): () => void {
    this.unwatch?.();
    this.unwatch = undefined;
    this.pages += 1;
    return () => {
      this.pages -= 1;
      queueMicrotask(() => {
        if (this.pages === 0 && !this.adopted) abandon();
      });
    };
  }

  adopt(): void {
    this.adopted = true;
  }

  drop(): void {
    this.dropped = true;
    this.unwatch?.();
    this.unwatch = undefined;
    for (const reference of this.held.splice(0)) reference.dispose();
  }
}

type LibraryInvalidation = {
  shelfId?: string;
  reloadId?: string;
  siblingEditions?: { id: string; workId: string };
};

/** Own one Navigation entry's intent, displayed source, isolated replacement and route references. */
export class LibraryVisitOwner {
  private state: VisitState = {
    source: undefined,
    candidate: undefined,
    candidateReady: false,
    error: null,
    restoreGeneration: 0,
    position: undefined,
  };
  private readonly listeners = new Set<() => void>();
  private readonly preparations = new Set<LibraryPreparation>();
  private settled: LibraryPreparation | undefined;
  private generation = 0;
  private readonly provisionalKey = crypto.randomUUID();
  private activation = "";
  private candidateRequest: Subscription | undefined;
  private restartedCursor = false;
  private disposed = false;
  private captureCurrent: (() => LibraryPosition | undefined) | undefined;
  readonly intent: LibraryIntent;
  readonly environment: Environment;

  constructor(environment: Environment, entryId: string | undefined, visit: LibraryVisit) {
    this.environment = environment;
    this.intent = new LibraryIntent(entryId, visit);
  }

  get entryId(): string | undefined {
    return this.intent.entryId;
  }

  bind(entryId: string): void {
    this.intent.bind(entryId);
  }

  private isCurrent(): boolean {
    return (
      !this.disposed && this.entryId !== undefined && navigation.currentEntry?.id === this.entryId
    );
  }

  begin(): LibraryPreparation {
    const preparation = new LibraryPreparation();
    if (this.disposed) preparation.drop();
    else this.preparations.add(preparation);
    return preparation;
  }

  settle(preparation: LibraryPreparation): void {
    if (this.settled !== undefined && this.settled !== preparation) {
      this.settled.drop();
      this.preparations.delete(this.settled);
    }
    this.settled = preparation;
  }

  discard(preparation: LibraryPreparation): void {
    preparation.drop();
    this.preparations.delete(preparation);
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  snapshot = (): VisitState => this.state;

  private publish(patch: Partial<VisitState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  private newSource(
    request: LibraryRequest,
    sort: readonly SortLevelParam[],
    storeOnly = false,
  ): LibrarySource {
    return new LibrarySource(
      this.environment,
      request,
      `${this.entryId ?? this.provisionalKey}:${String(++this.generation)}`,
      sort,
      storeOnly,
    );
  }

  prepare(request: LibraryRequest, sort: readonly SortLevelParam[]): LibrarySource {
    const source = this.state.source;
    if (source !== undefined && sameRequest(source.request, request))
      return new LibrarySource(this.environment, request, source.key, sort, true);
    return this.newSource(request, sort);
  }

  enter(initial: LibrarySource, position: LibraryPosition | undefined): void {
    if (!this.isCurrent()) return;
    const previous = this.state.source;
    const source = previous?.key === initial.key ? previous : undefined;
    const fresh = this.environment.check(initial.operation).status === "available";
    this.activation = "";
    this.cancelCandidate();
    this.publish({ position, error: null, restoreGeneration: this.state.restoreGeneration + 1 });
    if (!initial.request.eligible) return;
    if (source !== undefined) {
      if (!fresh) this.replace(initial.request, initial.sort, position);
    } else if (position !== undefined) {
      const candidate = new LibrarySource(
        this.environment,
        initial.request,
        initial.key,
        initial.sort,
        true,
      );
      this.publish({ candidate, candidateReady: true });
    } else {
      this.adopt(initial);
    }
  }

  capture(position: LibraryPosition): void {
    this.state = { ...this.state, position };
  }

  registerCapture(capture: () => LibraryPosition | undefined): () => void {
    this.captureCurrent = capture;
    return () => {
      if (this.captureCurrent === capture) this.captureCurrent = undefined;
    };
  }

  cancelRestoration(): void {
    if (this.state.position === undefined) return;
    if (this.state.source !== undefined) this.cancelCandidate();
    this.publish({ position: undefined, restoreGeneration: this.state.restoreGeneration + 1 });
  }

  leave(): void {
    this.cancelCandidate();
    this.captureCurrent = undefined;
    this.activation = "";
  }

  private cancelCandidate(): void {
    this.candidateRequest?.unsubscribe();
    this.candidateRequest = undefined;
    this.state.candidate?.dispose();
    if (this.state.candidate !== undefined)
      this.publish({ candidate: undefined, candidateReady: false });
  }

  activate(request: LibraryRequest, sort: readonly SortLevelParam[], intent: string): void {
    if (!this.isCurrent() || this.activation === intent) return;
    const changed = this.activation !== "";
    this.activation = intent;
    if (changed) {
      this.cancelCandidate();
      this.publish({ position: undefined, restoreGeneration: this.state.restoreGeneration + 1 });
    }
    if (!request.eligible) {
      this.cancelCandidate();
      return;
    }
    const current = this.state.candidate ?? this.state.source;
    if (current !== undefined && sameRequest(current.request, request)) return;
    this.replace(request, sort);
  }

  private replace(
    request: LibraryRequest,
    sort: readonly SortLevelParam[],
    position?: LibraryPosition,
  ): void {
    this.cancelCandidate();
    this.restartedCursor = false;
    const candidate = this.newSource(request, sort, true);
    this.publish({
      candidate,
      candidateReady: false,
      position,
      error: null,
      restoreGeneration: this.state.restoreGeneration + 1,
    });
    this.candidateRequest = fetchQuery<LibraryPageQuery>(
      this.environment,
      LibraryPageDocument,
      candidate.variables,
    ).subscribe({
      complete: () => {
        if (this.isCurrent() && this.state.candidate === candidate)
          this.publish({ candidateReady: true });
      },
      error: (error: Error) => {
        this.failed(candidate, error);
      },
    });
  }

  private adopt(candidate: LibrarySource): void {
    const previous = this.state.source;
    const source = new LibrarySource(
      this.environment,
      candidate.request,
      candidate.key,
      candidate.sort,
      true,
    );
    this.publish({ source, candidate: undefined, candidateReady: false, error: null });
    previous?.dispose();
  }

  pageCommitted(candidate: LibrarySource, ids: readonly string[], hasNext: boolean): boolean {
    if (!this.isCurrent() || this.state.candidate !== candidate) return false;
    const position = this.state.position;
    if (
      hasNext &&
      position !== undefined &&
      (ids.length < position.extent ||
        (position.anchorId !== null && !ids.includes(position.anchorId)))
    )
      return true;
    this.adopt(candidate);
    candidate.dispose();
    return false;
  }

  failed(candidate: LibrarySource, error: Error): void {
    if (!this.isCurrent() || this.state.candidate !== candidate) return;
    if (
      !this.restartedCursor &&
      error instanceof GraphQLExecutionError &&
      error.errors.some((failure) => failure.extensions?.code === "invalid-cursor")
    ) {
      this.replace(candidate.request, candidate.sort, this.state.position);
      this.restartedCursor = true;
      return;
    }
    this.cancelCandidate();
    this.publish({ error });
  }

  refresh(): void {
    const source = this.state.source;
    if (source === undefined || !this.isCurrent()) return;
    const position = this.captureCurrent?.() ?? this.state.position;
    this.replace(source.request, source.sort, position);
  }

  invalidate(
    fields: readonly string[],
    { shelfId, reloadId, siblingEditions }: LibraryInvalidation,
  ): void {
    const source = this.state.source;
    const candidate = this.state.candidate;
    const stale = new Set<LibrarySource>();
    for (const generation of [source, candidate]) {
      if (generation === undefined) continue;
      const matchesRequest = dependsOn(generation, fields, shelfId);
      if (!matchesRequest && reloadId === undefined && siblingEditions === undefined) continue;
      const { filter, sort } = generation.request;
      const id = ConnectionHandler.getConnectionID(ROOT_ID, "readerLibrary_books", {
        __dynamicKey: generation.key,
        filter,
        sort,
      });
      this.environment.commitUpdate((store) => {
        const connection = store.get(id);
        const record: ReadOnlyRecordProxy | null | undefined = connection;
        const affected =
          matchesRequest ||
          record?.getLinkedRecords("edges")?.some((edge) => {
            const book: ReadOnlyRecordProxy | null | undefined = edge?.getLinkedRecord("node");
            if (book == null) return false;
            return (
              book.getDataID() === reloadId ||
              (siblingEditions !== undefined &&
                book.getDataID() !== siblingEditions.id &&
                book.getLinkedRecord("work")?.getDataID() === siblingEditions.workId)
            );
          });
        if (affected === true) {
          stale.add(generation);
          connection?.invalidateRecord();
        }
      });
    }
    if (candidate !== undefined && this.isCurrent()) {
      // A pre-edit read can publish an older revision even if its sort and filters are unaffected.
      this.replace(candidate.request, candidate.sort, this.state.position);
    } else if (candidate === undefined && source !== undefined && stale.has(source)) {
      this.refresh();
    }
  }

  dispose(): void {
    this.disposed = true;
    this.cancelCandidate();
    this.state.source?.dispose();
    for (const preparation of this.preparations) preparation.drop();
    this.preparations.clear();
    this.settled = undefined;
    this.listeners.clear();
    this.state = { ...this.state, source: undefined, position: undefined };
  }
}

function sameRequest(left: LibraryRequest, right: LibraryRequest): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function dependsOn(source: LibrarySource, fields: readonly string[], shelfId?: string): boolean {
  const filter = source.request.filter;
  if (shelfId !== undefined)
    return filter?.shelf === shelfId || (filter?.shelfAny?.includes(shelfId) ?? false);
  return fields.some((field) => {
    const sortField = field === "contributors.author" || field === "authors" ? "author" : field;
    if (source.sort.some((level) => level.field === sortField)) return true;
    if (filter == null) return false;
    switch (sortField) {
      case "title":
        return (
          filter.q !== undefined ||
          filter.titleContains !== undefined ||
          filter.titleEq !== undefined ||
          filter.titleNe !== undefined
        );
      case "description":
        return filter.q !== undefined;
      case "subtitle":
        return filter.subtitleContains !== undefined || filter.subtitleEmpty !== undefined;
      case "author":
        return filter.author != null || filter.authorAny != null || filter.authorNone != null;
      case "isbn_13":
      case "isbn13":
        return (
          filter.isbn13Contains !== undefined ||
          filter.isbn13Eq !== undefined ||
          filter.isbn13Empty !== undefined
        );
      case "pages":
        return (
          filter.pagesGte !== undefined ||
          filter.pagesLte !== undefined ||
          filter.pagesEmpty !== undefined
        );
      case "status":
        return filter.statusAny != null || filter.statusNone != null;
      case "rating":
        return (
          filter.ratingGte !== undefined ||
          filter.ratingLte !== undefined ||
          filter.ratingEmpty !== undefined
        );
      case "genres":
        return filter.genre != null || filter.genreAny != null || filter.genreNone != null;
      case "moods":
        return filter.mood != null || filter.moodAny != null || filter.moodNone != null;
      case "tags":
        return filter.tag != null || filter.tagAny != null || filter.tagNone != null;
      default:
        return false;
    }
  });
}

const visits = new Map<string, LibraryVisitOwner>();
let provisional: LibraryVisitOwner | undefined;
let traversal: string | undefined;
let prepared = false;

if (typeof navigation !== "undefined")
  navigation.addEventListener("navigate", (event) => {
    traversal = event.navigationType === "traverse" ? event.destination.id : undefined;
  });

function track(entry: NavigationHistoryEntry, owner: LibraryVisitOwner): void {
  visits.set(entry.id, owner);
  entry.addEventListener(
    "dispose",
    () => {
      owner.dispose();
      visits.delete(entry.id);
    },
    { once: true },
  );
}

function sameLocation(entry: NavigationHistoryEntry, url: string): boolean {
  if (entry.url === null) return false;
  const current = new URL(entry.url);
  const destination = new URL(url);
  return current.pathname === destination.pathname && current.search === destination.search;
}

/**
 * Resolve the visit owner a Library route load prepares. A traversal or the document's first load
 * settles on its own entry and restores that entry's saved visit; any other destination gets a
 * provisional owner with no restored state, which the mounting page binds. A push runs its loader
 * before the browser records the new entry, so a traversal settles only the one load it reached.
 */
export function libraryDestination(
  environment: Environment,
  url: string,
  restore: () => LibraryVisit,
): LibraryVisitOwner {
  const entry = navigation.currentEntry;
  const first = !prepared;
  const traversed = traversal;
  prepared = true;
  traversal = undefined;
  if (
    entry !== null &&
    sameLocation(entry, url) &&
    (traversed === entry.id || (first && navigation.activation?.entry.id === entry.id))
  ) {
    const restored = restore();
    let owner = visits.get(entry.id);
    if (owner?.environment !== environment) {
      owner?.dispose();
      owner = new LibraryVisitOwner(environment, entry.id, restored);
      track(entry, owner);
    } else owner.intent.restore(restored);
    return owner;
  }
  provisional?.dispose();
  provisional = new LibraryVisitOwner(environment, undefined, { filters: emptyFilterState() });
  return provisional;
}

/** Bind a provisional owner to the entry its page mounted on. */
export function bindLibraryVisit(owner: LibraryVisitOwner): void {
  const entry = navigation.currentEntry;
  if (owner.entryId !== undefined || entry === null) return;
  if (provisional === owner) provisional = undefined;
  owner.bind(entry.id);
  track(entry, owner);
}

export function invalidateLibraryVisits(
  environment: Environment,
  fields: readonly string[],
  options: LibraryInvalidation = {},
): void {
  for (const owner of visits.values()) {
    if (owner.environment === environment) owner.invalidate(fields, options);
  }
}

export function clearLibraryVisits(): void {
  for (const owner of visits.values()) owner.dispose();
  visits.clear();
  provisional?.dispose();
  provisional = undefined;
}

export type LibraryLookups = { authorIds: string[]; seriesIds: string[]; shelfIds: string[] };

/** The author, series and shelf selections a visit's labels and restore checks look up. */
export function libraryLookups(filters: FilterState): LibraryLookups {
  return {
    authorIds: [
      ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
    ],
    seriesIds: [...new Set(filters.series)],
    shelfIds: [...new Set(filters.shelf)],
  };
}

/**
 * Reset a restored visit's filters and saved position when any selected author, series or shelf is
 * omitted by the server's lookups, preserving any view override.
 */
export function validateRestoredVisit(
  visit: LibraryVisit,
  lookups: LibraryLookups,
  visible: {
    authors: ReadonlySet<string>;
    series: ReadonlySet<string>;
    shelves: ReadonlySet<string>;
  },
): LibraryVisit {
  const missing =
    lookups.authorIds.some((id) => !visible.authors.has(id)) ||
    lookups.seriesIds.some((id) => !visible.series.has(id)) ||
    lookups.shelfIds.some((id) => !visible.shelves.has(id));
  if (!missing) return visit;
  return { ...visit, filters: emptyFilterState(), position: undefined };
}
