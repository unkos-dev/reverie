import { loadQuery, type PreloadedQuery } from "react-relay";
import { createOperationDescriptor, getRequest, type Environment } from "relay-runtime";

import {
  GraphQLExecutionError,
  decodeGraphQLId,
  executeEditingOnce,
  libraryVariables,
} from "@/api/graphql";
import type { ListBooksParams } from "@/api/books";
import type { LibraryPosition } from "@/routes/library-params";
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
          this.rows.set(decodeGraphQLId("Manifestation", edge.node.id), {
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

/** Own one Navigation entry's displayed source and isolated replacement. */
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
  private intent = "";
  private activation = "";
  private candidate: LibrarySource | undefined;
  private returning = false;
  private disposed = false;
  private desired: ListBooksParams | undefined;
  private eligible = false;
  private captureCurrent: (() => LibraryPosition | undefined) | undefined;
  readonly environment: Environment;
  readonly entryKey: string;

  constructor(environment: Environment, entryKey: string) {
    this.environment = environment;
    this.entryKey = entryKey;
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
    const returning = this.returning;
    const activation = JSON.stringify([params, intent, eligible]);
    if (!returning && activation === this.activation) return;
    this.activation = activation;
    this.returning = false;
    this.desired = structuredClone(params);
    this.eligible = eligible;
    const changedIntent = intent !== this.intent;
    this.intent = intent;
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

  private async replace(
    params: ListBooksParams,
    position?: LibraryPosition,
    retainedExtent = position?.extent ?? 0,
    restartCursor = true,
  ): Promise<void> {
    this.cancelCandidate();
    const request = this.request;
    const intent = this.intent;
    const source = new LibrarySource(
      this.environment,
      structuredClone(params),
      `${this.entryKey}:${String(++this.generation)}`,
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
      } while (
        source.hasNext &&
        (source.rows.size < retainedExtent ||
          (position?.anchorId !== null &&
            position?.anchorId !== undefined &&
            !source.rows.has(position.anchorId)))
      );
      if (!this.current(request, intent)) return;
      const previous = this.state.source;
      this.candidate = undefined;
      this.publish({ source, pending: false, error: null, moreError: null });
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
      !this.disposed &&
      request === this.request &&
      intent === this.intent &&
      navigation.currentEntry?.key === this.entryKey
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

export function libraryVisit(environment: Environment): LibraryVisitOwner {
  const entry = navigation.currentEntry;
  if (entry === null) throw new Error("A Library Navigation entry is required.");
  const existing = visits.get(entry.key);
  if (existing !== undefined) return existing;
  const owner = new LibraryVisitOwner(environment, entry.key);
  visits.set(entry.key, owner);
  entry.addEventListener(
    "dispose",
    () => {
      owner.dispose();
      visits.delete(entry.key);
    },
    { once: true },
  );
  return owner;
}

export function clearLibraryVisits(): void {
  for (const owner of visits.values()) owner.dispose();
  visits.clear();
}
