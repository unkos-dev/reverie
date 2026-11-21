import { loadQuery, type PreloadedQuery } from "react-relay";
import { createOperationDescriptor, getRequest, type Environment } from "relay-runtime";

import { GraphQLExecutionError, executeEditingOnce, libraryVariables } from "@/api/graphql";
import type { ListBooksParams } from "@/api/books";
import { LibraryIntent } from "@/lib/hooks/use-library-filters";
import {
  emptyFilterState,
  type FilterState,
  type LibraryPosition,
  type LibraryVisit,
} from "@/routes/library-params";
import { readerLibraryQuery } from "./reader";
import type { readerLibraryQuery as LibraryQuery } from "./__generated__/readerLibraryQuery.graphql";

export type LibraryRowSnapshot = Pick<
  LibraryQuery["rawResponse"]["books"]["edges"][number]["node"],
  "metadata" | "readingState"
>;

/** One native connection generation with detached editing snapshots and explicit retention. */
export class LibrarySource {
  readonly rows = new Map<string, LibraryRowSnapshot>();
  controller = new AbortController();
  readonly variables: LibraryQuery["variables"];
  reference: PreloadedQuery<LibraryQuery> | undefined;
  cursor: string | null = null;
  hasNext = true;
  private readonly initialRetention;
  private uses = 0;
  private retired = false;
  private disposed = false;

  readonly environment: Environment;
  readonly params: ListBooksParams;
  readonly key: string;
  private readonly released: () => void;

  constructor(
    environment: Environment,
    params: ListBooksParams,
    key: string,
    released: () => void,
  ) {
    this.environment = environment;
    this.params = params;
    this.key = key;
    this.released = released;
    this.variables = libraryVariables(params, key);
    this.initialRetention = environment.retain(
      createOperationDescriptor(getRequest(readerLibraryQuery), this.variables),
    );
  }

  async loadPage(): Promise<boolean> {
    const controller = this.controller;
    const data = await executeEditingOnce<LibraryQuery>(
      this.environment,
      readerLibraryQuery,
      {
        ...this.variables,
        cursor: this.cursor,
      },
      controller.signal,
      (payload) => {
        for (const edge of payload.books.edges)
          this.rows.set(edge.node.id, {
            metadata: edge.node.metadata,
            readingState: edge.node.readingState,
          });
        this.cursor = payload.books.pageInfo.endCursor ?? null;
        this.hasNext = payload.books.pageInfo.hasNextPage;
      },
    );
    if (data === undefined || controller.signal.aborted) return false;
    if (this.reference === undefined) {
      this.reference = loadQuery<LibraryQuery>(
        this.environment,
        readerLibraryQuery,
        this.variables,
        { fetchPolicy: "store-only" },
      );
      this.initialRetention.dispose();
    }
    return true;
  }

  acquire(): () => void {
    this.uses += 1;
    let held = true;
    return () => {
      if (!held) return;
      held = false;
      this.uses -= 1;
      if (this.retired && this.uses === 0) this.dispose();
    };
  }

  pause(): void {
    this.controller.abort();
    this.controller = new AbortController();
  }

  retire(): void {
    this.retired = true;
    this.controller.abort();
    if (this.uses === 0) this.dispose();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.controller.abort();
    this.reference?.dispose();
    this.initialRetention.dispose();
    this.rows.clear();
    this.released();
  }
}

type VisitState = {
  source: LibrarySource | undefined;
  pending: boolean;
  loadingMore: boolean;
  error: Error | null;
  moreError: Error | null;
  restoreGeneration: number;
  position: LibraryPosition | undefined;
};

type Disposable = { dispose: () => void };

/** The query references one route load created, owned from the moment each exists. */
export class LibraryPreparation {
  private readonly held: Disposable[] = [];
  private dropped = false;

  hold<T extends Disposable>(reference: T): T {
    if (this.dropped) reference.dispose();
    else this.held.push(reference);
    return reference;
  }

  drop(): void {
    this.dropped = true;
    for (const reference of this.held.splice(0)) reference.dispose();
  }
}

/** Own one Navigation entry's intent, displayed source, isolated replacement and route references. */
export class LibraryVisitOwner {
  private state: VisitState = {
    source: undefined,
    pending: false,
    loadingMore: false,
    error: null,
    moreError: null,
    restoreGeneration: 0,
    position: undefined,
  };
  private readonly listeners = new Set<() => void>();
  private readonly sources = new Set<LibrarySource>();
  private generation = 0;
  private request = 0;
  private intentToken = "";
  private activation = "";
  private candidate: LibrarySource | undefined;
  private returning = false;
  private disposed = false;
  private desired: ListBooksParams | undefined;
  private eligible = false;
  private captureCurrent: (() => LibraryPosition | undefined) | undefined;
  private readonly preparations = new Set<LibraryPreparation>();
  private settled: LibraryPreparation | undefined;
  readonly environment: Environment;
  readonly intent: LibraryIntent;

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
    return this.entryId !== undefined && navigation.currentEntry?.id === this.entryId;
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

  enter(position: LibraryPosition | undefined): void {
    this.returning = this.state.source !== undefined || position !== undefined;
    this.publish({ position, restoreGeneration: this.state.restoreGeneration + 1 });
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
    const restart = this.state.source === undefined ? this.candidate?.params : undefined;
    this.cancelCandidate();
    this.publish({
      position: undefined,
      pending: false,
      restoreGeneration: this.state.restoreGeneration + 1,
    });
    if (restart !== undefined) void this.replace(restart);
  }

  leave(): void {
    this.cancelCandidate();
    this.state.source?.pause();
    this.captureCurrent = undefined;
    this.activation = "";
  }

  private cancelCandidate(): void {
    this.request += 1;
    this.candidate?.retire();
    this.candidate = undefined;
  }

  activate(params: ListBooksParams, intent: string, eligible: boolean): void {
    if (!this.isCurrent()) return;
    const returning = this.returning;
    const activation = JSON.stringify([params, intent, eligible]);
    if (!returning && activation === this.activation) return;
    this.activation = activation;
    this.returning = false;
    this.desired = structuredClone(params);
    this.eligible = eligible;
    const changedIntent = intent !== this.intentToken;
    this.intentToken = intent;
    if (changedIntent) {
      this.cancelCandidate();
      if (!returning)
        this.publish({ position: undefined, restoreGeneration: this.state.restoreGeneration + 1 });
    }
    if (!eligible) {
      this.cancelCandidate();
      this.publish({ pending: false });
      return;
    }
    if (!returning && JSON.stringify(params) === JSON.stringify(this.state.source?.params)) {
      this.publish({ pending: false, error: null });
      return;
    }
    void this.replace(params, returning ? this.state.position : undefined);
  }

  refresh = (): Promise<void> => {
    const source = this.state.source;
    if (source === undefined || this.desired === undefined || !this.eligible)
      return Promise.resolve();
    const sameArguments = JSON.stringify(source.params) === JSON.stringify(this.desired);
    return this.replace(
      this.desired,
      sameArguments ? this.captureCurrent?.() : undefined,
      sameArguments ? source.rows.size : 0,
    );
  };

  private needsRetainedRows(
    source: LibrarySource,
    retainedExtent: number,
    position: LibraryPosition | undefined,
  ): boolean {
    return (
      source.rows.size < retainedExtent ||
      (position?.anchorId !== null &&
        position?.anchorId !== undefined &&
        !source.rows.has(position.anchorId))
    );
  }

  private async replace(
    params: ListBooksParams,
    position?: LibraryPosition,
    retainedExtent = position?.extent ?? 0,
    restartCursor = true,
  ): Promise<void> {
    this.cancelCandidate();
    const request = this.request;
    const intent = this.intentToken;
    const source = new LibrarySource(
      this.environment,
      structuredClone(params),
      `${String(this.entryId)}:${String(++this.generation)}`,
      () => {
        this.sources.delete(source);
      },
    );
    this.sources.add(source);
    this.candidate = source;
    this.publish({
      pending: true,
      error: null,
      position,
      restoreGeneration: this.state.restoreGeneration + 1,
    });
    try {
      do {
        if (!(await source.loadPage()) || !this.current(request, intent)) return;
      } while (source.hasNext && this.needsRetainedRows(source, retainedExtent, position));
      if (!this.current(request, intent)) return;
      const previous = this.state.source;
      this.candidate = undefined;
      this.publish({ source, pending: false, loadingMore: false, error: null, moreError: null });
      previous?.retire();
    } catch (error) {
      if (!this.current(request, intent)) return;
      source.retire();
      this.candidate = undefined;
      if (
        restartCursor &&
        source.cursor !== null &&
        error instanceof GraphQLExecutionError &&
        error.errors.some(
          (failure) =>
            failure.extensions?.code === "validation" && failure.message === "Invalid cursor.",
        )
      ) {
        await this.replace(params, position, retainedExtent, false);
        return;
      }
      if (this.state.source !== undefined)
        console.error("[LibraryPage] replacement fetch failed", error);
      this.publish({
        pending: false,
        error: error instanceof Error ? error : new Error("Library request failed."),
      });
    }
  }

  private current(request: number, intent: string): boolean {
    return (
      !this.disposed && request === this.request && intent === this.intentToken && this.isCurrent()
    );
  }

  loadMore = async (): Promise<void> => {
    const source = this.state.source;
    if (
      source === undefined ||
      !source.hasNext ||
      this.state.loadingMore ||
      this.state.pending ||
      this.returning
    )
      return;
    this.publish({ loadingMore: true, moreError: null });
    try {
      await source.loadPage();
      if (this.state.source === source && !this.disposed) this.publish({ loadingMore: false });
    } catch (error) {
      if (this.state.source === source && !this.disposed) {
        console.error("[LibraryPage] page fetch failed", error);
        this.publish({
          loadingMore: false,
          moreError: error instanceof Error ? error : new Error("Library page failed."),
        });
      }
    }
  };

  dispose(): void {
    this.disposed = true;
    this.cancelCandidate();
    for (const source of this.sources) source.dispose();
    this.sources.clear();
    for (const preparation of this.preparations) preparation.drop();
    this.preparations.clear();
    this.settled = undefined;
    this.state = {
      ...this.state,
      source: undefined,
      position: undefined,
      pending: false,
      loadingMore: false,
    };
    this.listeners.clear();
  }
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
 * Drop each restored author, series and shelf selection the server's lookups omit. Anything
 * dropped also clears the saved position, as every filter change does.
 */
export function pruneVisit(
  visit: LibraryVisit,
  visible: {
    authors: ReadonlySet<string>;
    series: ReadonlySet<string>;
    shelves: ReadonlySet<string>;
  },
): LibraryVisit {
  const { authors, series, shelf } = visit.filters;
  const keep = (ids: readonly string[], found: ReadonlySet<string>) =>
    ids.filter((id) => found.has(id));
  const next = {
    authors: {
      all: keep(authors.all, visible.authors),
      any: keep(authors.any, visible.authors),
      none: keep(authors.none, visible.authors),
    },
    series: keep(series, visible.series),
    shelf: keep(shelf, visible.shelves),
  };
  const dropped =
    next.authors.all.length + next.authors.any.length + next.authors.none.length <
      authors.all.length + authors.any.length + authors.none.length ||
    next.series.length < series.length ||
    next.shelf.length < shelf.length;
  if (!dropped) return visit;
  return { ...visit, filters: { ...visit.filters, ...next }, position: undefined };
}
