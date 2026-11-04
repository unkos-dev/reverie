/**
 * Cell-edit orchestration for the library table.
 *
 * `EDIT_ROUTES` describes how a column writes; this hook is the plumbing
 * that runs on top of it, including deriving each edit's undo strategy:
 * firing the right mutation for a committed cell edit, patching the
 * TanStack Query list cache from the server's applied value (never the
 * local draft, since the server normalizes input, e.g. ISBN checksums),
 * tracking in-flight cells so the table can render a draft overlay, and a
 * bounded sequential undo stack. The query cache stays
 * server-confirmed throughout: there is no optimistic commit.
 *
 * `listKey` must be the exact key the library page's displayed query reads
 * (cursor stripped). The caller owns computing that key; this hook only
 * consumes it. A mismatched key silently writes a dead cache entry.
 */
import {
  useMutation,
  useMutationState,
  useQueries,
  queryOptions,
  type Mutation,
  type QueryKey,
  useQueryClient,
  type InfiniteData,
  type QueryClient,
} from "@tanstack/react-query";
import { useRef, useState, type KeyboardEvent } from "react";
import type { AuthMe } from "@/hooks/useAuthMe";

import {
  ApiError,
  getBookMetadata,
  isIfMatchMismatch,
  isIfMatchRequired,
  revertField,
  updateBookMetadata,
  type BookMetadata,
  type BookListItem,
  type BookListResponse,
  type UpdateBookMetadataFields,
} from "@/api";
import type { FieldVersionChange } from "@/api/books";
import {
  getReadingState,
  updateReadingState,
  type ReadingState,
  type UpdateReadingFields,
} from "@/api/reading";
import type { CellEditReport, GridColumn } from "@/lib/grid/types";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import { EDIT_ROUTES, type EditableColumnKey, type EditRoute } from "./edit-routing";

const COLUMN_LABELS: Readonly<Record<EditableColumnKey, string>> = {
  title: "Title",
  subtitle: "Subtitle",
  isbn_13: "ISBN",
  pages: "Pages",
  authors: "Authors",
  status: "Status",
  rating: "Rating",
};

const UNDO_STACK_CAP = 10;

type MetadataColumnKey = Extract<
  EditableColumnKey,
  "title" | "subtitle" | "isbn_13" | "pages" | "authors"
>;
type ReadingColumnKey = Extract<EditableColumnKey, "status" | "rating">;

function isEditableColumnKey(key: string): key is EditableColumnKey {
  return key in EDIT_ROUTES;
}

function isMetadataColumnKey(key: EditableColumnKey): key is MetadataColumnKey {
  return EDIT_ROUTES[key].pipeline === "metadata";
}

function isReadingColumnKey(key: EditableColumnKey): key is ReadingColumnKey {
  return EDIT_ROUTES[key].pipeline === "reading";
}

/** One undo-stack entry. `previousRow` is the full pre-edit snapshot so a
 *  metadata undo can restore the cache directly rather than re-deriving it. */
type MetadataUndoEntry =
  | {
      id: string;
      pipeline: "metadata";
      strategy: "revert";
      manifestationId: string;
      columnKey: MetadataColumnKey;
      fieldName: string;
      previousVersionId: string | null;
      previousRow: BookListItem;
    }
  | {
      id: string;
      pipeline: "metadata";
      strategy: "counter-patch";
      manifestationId: string;
      columnKey: MetadataColumnKey;
      counterPatch: UpdateBookMetadataFields;
      previousRow: BookListItem;
    };

type ReadingUndoEntry = {
  id: string;
  pipeline: "reading";
  manifestationId: string;
  columnKey: ReadingColumnKey;
  counterPatch: UpdateReadingFields;
  previousRow: BookListItem;
};

type UndoEntry = (MetadataUndoEntry | ReadingUndoEntry) & { listKey: BooksListKey };

/** `${row.id}:${columnKey}`, the shared key shape for the pending-cells map. */
export function pendingKey(rowId: string, columnKey: string): string {
  return `${rowId}:${columnKey}`;
}

/**
 * `response.fields` types as `Record<string, FieldVersionChange>`, a
 * compile-time promise that every key exists. The server only actually
 * populates it per touched field, so a route's own field can be absent at
 * runtime despite the type. Reading through this narrower boundary (rather
 * than indexing `response.fields` directly at the call site) keeps the
 * caller's missing-field guard a real runtime check instead of a condition
 * the type checker can prove impossible.
 */
function lookupAppliedField(
  fields: Record<string, FieldVersionChange>,
  field: string,
): FieldVersionChange | undefined {
  return fields[field];
}

/** True when `item` is the edited manifestation itself, or a sibling
 *  edition of the same work on a field that fans out across editions. */
function affectsManifestation(
  item: BookListItem,
  manifestationId: string,
  workId: string,
  workScoped: boolean,
): boolean {
  return item.id === manifestationId || (workScoped && item.work_id === workId);
}

function displayValue(
  columns: readonly GridColumn<BookListItem>[],
  columnKey: string,
  row: BookListItem,
): string {
  const column = columns.find((c) => c.key === columnKey);
  return column === undefined ? "" : column.accessor(row);
}

/**
 * Builds the patch a commit would send, or `null` when the committed row
 * is unchanged for this column (a no-op edit fires no mutation). Comparing
 * against the patch that `toPatch` would build from the row's own prior
 * value keeps this generic over every route's patch shape.
 */
function buildPatchIfChanged<P>(
  route: { toPatch: (row: BookListItem, draft: BookListItem) => P },
  previousRow: BookListItem,
  row: BookListItem,
): P | null {
  const patch = route.toPatch(previousRow, row);
  const noop = route.toPatch(previousRow, previousRow);
  return JSON.stringify(patch) === JSON.stringify(noop) ? null : patch;
}

function updateListCache(
  queryClient: QueryClient,
  listKey: BooksListKey,
  shouldPatch: (row: BookListItem) => boolean,
  patch: (row: BookListItem) => BookListItem,
): void {
  queryClient.setQueryData<InfiniteData<BookListResponse>>(listKey, (old) => {
    if (old === undefined) return old;
    return {
      pageParams: old.pageParams,
      pages: old.pages.map((page) => ({
        ...page,
        items: page.items.map((item) => (shouldPatch(item) ? patch(item) : item)),
      })),
    };
  });
}

/** Invalidates the book-detail caches a write may have left stale. A
 *  work-scoped field can change editions the list cache has never loaded
 *  (opened directly, on an unloaded page, or excluded by the active
 *  filter), so it invalidates the whole detail family: inactive entries
 *  are only marked stale, and at most one detail view is on screen to
 *  refetch. Manifestation-scoped fields touch exactly one detail. */
function invalidateAffectedDetails(
  queryClient: QueryClient,
  manifestationId: string,
  workScoped: boolean,
): void {
  if (workScoped) {
    void queryClient.invalidateQueries({ queryKey: queryKeys.books.detailsAll });
    return;
  }
  void queryClient.invalidateQueries({ queryKey: queryKeys.books.detail(manifestationId) });
}

/**
 * Chooses the undo strategy for a metadata field. Title can never lack a
 * previous version once set (guarded defensively: the invariant should
 * hold, but undo is disabled rather than trusted blindly); authors falls
 * back to a counter-patch when the response carries no previous pointer
 * (unstamped `work_authors` rows), unless the work was authorless before
 * this edit, in which case undo is disabled the same way title's is (the
 * counter-patch would restore an empty author list, which the metadata
 * endpoint's last-author guard always rejects); every other field reverts
 * with `previous_version_id`, including `null`, which the revert endpoint
 * reads as "clear."
 */
function buildMetadataUndoEntry(
  id: string,
  columnKey: MetadataColumnKey,
  route: Extract<EditRoute, { pipeline: "metadata" }>,
  row: BookListItem,
  previousRow: BookListItem,
  applied: FieldVersionChange,
): MetadataUndoEntry | null {
  if (route.field === "title" && applied.previous_version_id === null) return null;
  if (route.field === "contributors.author" && applied.previous_version_id === null) {
    if (previousRow.authors.length === 0) return null;
    return {
      id,
      pipeline: "metadata",
      strategy: "counter-patch",
      manifestationId: row.id,
      columnKey,
      counterPatch: route.toPatch(row, previousRow),
      previousRow,
    };
  }
  return {
    id,
    pipeline: "metadata",
    strategy: "revert",
    manifestationId: row.id,
    columnKey,
    fieldName: route.field,
    previousVersionId: applied.previous_version_id,
    previousRow,
  };
}

/** Restores the exact pre-edit value from the snapshot, fanning out to
 *  loaded sibling editions for a work-scoped field. `applyToRow` is reused
 *  with a synthetic, unversioned `FieldVersionChange` purely for its
 *  per-column value coercion. */
function restoreMetadataRow(
  queryClient: QueryClient,
  listKey: BooksListKey,
  entry: MetadataUndoEntry,
): void {
  const route = EDIT_ROUTES[entry.columnKey];
  if (route.pipeline !== "metadata") return;
  const value: unknown = entry.previousRow[entry.columnKey];
  updateListCache(
    queryClient,
    listKey,
    (item) =>
      affectsManifestation(
        item,
        entry.manifestationId,
        entry.previousRow.work_id,
        route.workScoped,
      ),
    (item) => route.applyToRow(item, { value, version_id: null, previous_version_id: null }),
  );
}

/** Reading undo applies the counter-patch's own response (never the
 *  snapshot) since it can carry server-computed side effects such as
 *  transition stamps. */
function restoreReadingRow(
  queryClient: QueryClient,
  listKey: BooksListKey,
  entry: ReadingUndoEntry,
  response: ReadingState,
): void {
  const route = EDIT_ROUTES[entry.columnKey];
  if (route.pipeline !== "reading") return;
  updateListCache(
    queryClient,
    listKey,
    (item) => item.id === entry.manifestationId,
    (item) => route.applyToRow(item, response),
  );
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

function formatError(err: unknown): string {
  if (err instanceof ApiError) return `${err.title}: ${err.detail}`;
  if (err instanceof Error) return err.message;
  return "Request failed.";
}

type CellEditContext = {
  userId: string | null;
  operation: "save" | "undo";
  row: BookListItem;
  bookTitle: string;
  columnKey: EditableColumnKey;
  pipeline: "metadata" | "reading";
  workScoped: boolean;
  value: unknown;
  display: string;
  listKey: BooksListKey;
};

type CellEditRecord = {
  id: number;
  status: "idle" | "pending" | "success" | "error";
  submittedAt: number;
  error: Error | null;
  context: CellEditContext | undefined;
};

function cellEditKey(userId: string | null) {
  return ["library-cell-edit", userId] as const;
}

function resourceKey(context: CellEditContext): QueryKey {
  return context.pipeline === "metadata"
    ? queryKeys.books.metadata(context.row.id)
    : queryKeys.books.reading(context.row.id, context.userId);
}

function valueForRow(row: BookListItem, columnKey: EditableColumnKey): unknown {
  return isReadingColumnKey(columnKey) ? (row.reading_state?.[columnKey] ?? null) : row[columnKey];
}

function readValue(context: CellEditContext, data: BookMetadata | ReadingState): unknown {
  if ("contributors" in data && isMetadataColumnKey(context.columnKey)) {
    return context.columnKey === "authors" ? data.contributors.author : data[context.columnKey];
  }
  if ("status" in data && isReadingColumnKey(context.columnKey)) return data[context.columnKey];
  return undefined;
}

function matchesRead(context: CellEditContext, data: BookMetadata | ReadingState): boolean {
  const value = readValue(context, data);
  const submitted = context.value;
  if (context.columnKey === "isbn_13" && typeof value === "string" && typeof submitted === "string")
    return value === submitted.replace(/[ -]/g, "");
  return JSON.stringify(value) === JSON.stringify(submitted);
}

function recoveryOptions(id: number, context: CellEditContext, client: QueryClient) {
  return queryOptions({
    queryKey: [...resourceKey(context), "cell-recovery", id],
    queryFn: async ({ signal }): Promise<BookMetadata | ReadingState> => {
      const data = await (context.pipeline === "metadata"
        ? getBookMetadata(context.row.id, signal)
        : getReadingState(context.row.id, signal));
      applyRecoveryRead(client, context, data);
      return data;
    },
    enabled: (query) => query.state.data === undefined || !matchesRead(context, query.state.data),
    staleTime: 0,
  });
}

function applyRecoveryRead(
  client: QueryClient,
  context: CellEditContext,
  data: BookMetadata | ReadingState,
): void {
  client.setQueryData(resourceKey(context), data);
  const route = EDIT_ROUTES[context.columnKey];
  if (route.pipeline === "metadata" && "contributors" in data) {
    const value = readValue(context, data);
    updateListCache(
      client,
      context.listKey,
      (row) => affectsManifestation(row, context.row.id, context.row.work_id, route.workScoped),
      (row) => route.applyToRow(row, { value, version_id: null, previous_version_id: null }),
    );
  } else if (route.pipeline === "reading" && "status" in data) {
    updateListCache(
      client,
      context.listKey,
      (row) => row.id === context.row.id,
      (row) => route.applyToRow(row, data),
    );
  }
  void client.invalidateQueries({ queryKey: context.listKey });
  invalidateAffectedDetails(client, context.row.id, context.workScoped);
}

function useCellEditRecords(): CellEditRecord[] {
  const client = useQueryClient();
  const userId = client.getQueryData<AuthMe>(queryKeys.auth.me())?.id ?? null;
  return useMutationState<CellEditRecord, Mutation<unknown, Error, { context: CellEditContext }>>({
    filters: { mutationKey: cellEditKey(userId) },
    select: (mutation) => ({
      id: mutation.mutationId,
      status: mutation.state.status,
      submittedAt: mutation.state.submittedAt,
      error: mutation.state.error,
      context: mutation.state.variables?.context,
    }),
  });
}

type CellEditFeedback = {
  id: number;
  bookTitle: string;
  field: string;
  submitted: string;
  message: string;
  reloading: boolean;
  reload: () => void;
};

export function useCellEditFeedback(): CellEditFeedback[] {
  const client = useQueryClient();
  const records = useCellEditRecords();
  const errors = records.flatMap((record) => {
    const context = record.context;
    if (
      record.status !== "error" ||
      context === undefined ||
      records.some(
        (later) =>
          later.status === "success" &&
          later.submittedAt > record.submittedAt &&
          later.context?.row.id === context.row.id &&
          later.context.columnKey === context.columnKey,
      )
    )
      return [];
    return [{ ...record, context }];
  });
  const recovery = useQueries({
    queries: errors.map((record) => recoveryOptions(record.id, record.context, client)),
  });
  return errors.flatMap((record, index) => {
    const context = record.context;
    const read = recovery[index];
    if (read.isSuccess && matchesRead(context, read.data)) return [];
    let message = formatError(record.error);
    if (isIfMatchMismatch(record.error) || isIfMatchRequired(record.error))
      message = "This book changed elsewhere. Reload the latest values before retrying.";
    else if (
      context.columnKey === "isbn_13" &&
      record.error instanceof ApiError &&
      record.error.status === 422 &&
      record.error.errors?.isbn_13 !== undefined
    )
      message = "ISBN-13 wasn’t saved. Check the 13-digit number; spaces and hyphens are allowed.";
    if (read.isError) message += " Could not reload the current value. Try Reload again.";
    let submitted = JSON.stringify(context.value);
    if (context.value === null) submitted = "unset";
    else if (typeof context.value === "string") submitted = context.value;
    return [
      {
        id: record.id,
        bookTitle: context.bookTitle,
        field: COLUMN_LABELS[context.columnKey],
        submitted,
        message: context.operation === "undo" ? `Undo failed: ${message}` : message,
        reloading: read.isFetching,
        reload: () => {
          void read.refetch();
        },
      },
    ];
  });
}

export type UseCellEditOptions = {
  listKey: BooksListKey;
  columns: readonly GridColumn<BookListItem>[];
};

export type UseCellEditResult = {
  pendingCells: ReadonlyMap<string, string>;
  onCellEdit: (report: CellEditReport<BookListItem>) => void;
  onGridKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  undo: () => void;
  undoPending: boolean;
  undoAvailable: boolean;
  undoLabel: string | undefined;
};

export function useCellEdit({ listKey, columns }: UseCellEditOptions): UseCellEditResult {
  const queryClient = useQueryClient();
  const userId = queryClient.getQueryData<AuthMe>(queryKeys.auth.me())?.id ?? null;
  const [pendingCells, setPendingCells] = useState<ReadonlyMap<string, string>>(new Map());
  const [undoStack, setUndoStack] = useState<UndoEntry[]>([]);
  const undoIdCounter = useRef(0);

  function setPending(key: string, value: string): void {
    setPendingCells((prev) => {
      const next = new Map(prev);
      next.set(key, value);
      return next;
    });
  }

  function clearPending(key: string): void {
    setPendingCells((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Map(prev);
      next.delete(key);
      return next;
    });
  }

  function nextUndoId(): string {
    undoIdCounter.current += 1;
    return `undo-${String(undoIdCounter.current)}`;
  }

  function pushUndo(entry: UndoEntry): void {
    setUndoStack((stack) => [...stack, entry].slice(-UNDO_STACK_CAP));
  }

  function failed(error: Error, variables: { context: CellEditContext }): void {
    console.error("[useCellEdit] request failed", error);
    void queryClient.invalidateQueries({ queryKey: variables.context.listKey });
  }

  const metadataMutation = useMutation({
    mutationKey: cellEditKey(userId),
    mutationFn: async ({
      context,
      patch,
    }: {
      context: CellEditContext & { columnKey: MetadataColumnKey };
      patch: UpdateBookMetadataFields;
      previousRow: BookListItem;
    }) => {
      const response = await updateBookMetadata(context.row.id, patch);
      const route = EDIT_ROUTES[context.columnKey];
      const applied =
        route.pipeline === "metadata"
          ? lookupAppliedField(response.fields, route.field)
          : undefined;
      if (applied === undefined)
        throw new Error("The server response was missing the edited field.");
      return applied;
    },
    onSuccess: (applied, { context, previousRow }) => {
      const route = EDIT_ROUTES[context.columnKey];
      if (route.pipeline !== "metadata") return;
      updateListCache(
        queryClient,
        context.listKey,
        (item) => affectsManifestation(item, context.row.id, context.row.work_id, route.workScoped),
        (item) => route.applyToRow(item, applied),
      );
      invalidateAffectedDetails(queryClient, context.row.id, route.workScoped);
      const entry = buildMetadataUndoEntry(
        nextUndoId(),
        context.columnKey,
        route,
        context.row,
        previousRow,
        applied,
      );
      if (entry !== null) pushUndo({ ...entry, listKey: context.listKey });
    },
    onError: failed,
    onSettled: (_data, _error, { context }) => {
      clearPending(pendingKey(context.row.id, context.columnKey));
    },
  });

  const readingMutation = useMutation({
    mutationKey: cellEditKey(userId),
    mutationFn: ({
      context,
      patch,
    }: {
      context: CellEditContext & { columnKey: ReadingColumnKey };
      patch: UpdateReadingFields;
      previousRow: BookListItem;
    }) => updateReadingState(context.row.id, patch),
    onSuccess: (response, { context, previousRow }) => {
      const route = EDIT_ROUTES[context.columnKey];
      if (route.pipeline !== "reading") return;
      updateListCache(
        queryClient,
        context.listKey,
        (item) => item.id === context.row.id,
        (item) => route.applyToRow(item, response),
      );
      pushUndo({
        id: nextUndoId(),
        pipeline: "reading",
        manifestationId: context.row.id,
        columnKey: context.columnKey,
        counterPatch: route.toPatch(context.row, previousRow),
        previousRow,
        listKey: context.listKey,
      });
    },
    onError: failed,
    onSettled: (_data, _error, { context }) => {
      clearPending(pendingKey(context.row.id, context.columnKey));
    },
  });

  const undoMutation = useMutation({
    mutationKey: cellEditKey(userId),
    mutationFn: async ({ entry }: { context: CellEditContext; entry: UndoEntry }) => {
      if (entry.pipeline === "reading")
        return {
          pipeline: "reading",
          response: await updateReadingState(entry.manifestationId, entry.counterPatch),
        } as const;
      if (entry.strategy === "revert")
        await revertField(entry.manifestationId, entry.fieldName, entry.previousVersionId);
      else await updateBookMetadata(entry.manifestationId, entry.counterPatch);
      return { pipeline: "metadata" } as const;
    },
    onSuccess: (result, { entry }) => {
      if (entry.pipeline === "metadata") restoreMetadataRow(queryClient, entry.listKey, entry);
      else if (result.pipeline === "reading")
        restoreReadingRow(queryClient, entry.listKey, entry, result.response);
      const route = EDIT_ROUTES[entry.columnKey];
      invalidateAffectedDetails(
        queryClient,
        entry.manifestationId,
        route.pipeline === "metadata" && route.workScoped,
      );
      setUndoStack((stack) => stack.filter((item) => item.id !== entry.id));
    },
    onError: failed,
  });

  function onCellEdit({ row, previousRow, columnKey }: CellEditReport<BookListItem>): void {
    if (!isEditableColumnKey(columnKey)) return;
    const route = EDIT_ROUTES[columnKey];
    const context = {
      userId,
      operation: "save" as const,
      row,
      bookTitle: previousRow.title,
      pipeline: route.pipeline,
      workScoped: route.pipeline === "metadata" && route.workScoped,
      value: valueForRow(row, columnKey),
      display: displayValue(columns, columnKey, row),
      listKey,
    };
    if (route.pipeline === "metadata" && isMetadataColumnKey(columnKey)) {
      const patch = buildPatchIfChanged(route, previousRow, row);
      if (patch !== null) {
        setPending(pendingKey(row.id, columnKey), context.display);
        metadataMutation.mutate({ context: { ...context, columnKey }, patch, previousRow });
      }
    } else if (route.pipeline === "reading" && isReadingColumnKey(columnKey)) {
      const patch = buildPatchIfChanged(route, previousRow, row);
      if (patch !== null) {
        setPending(pendingKey(row.id, columnKey), context.display);
        readingMutation.mutate({ context: { ...context, columnKey }, patch, previousRow });
      }
    }
  }

  const latest = undoStack.at(-1);
  const undoAvailable = latest !== undefined && pendingCells.size === 0 && !undoMutation.isPending;
  function undo(): void {
    if (!undoAvailable) return;
    const route = EDIT_ROUTES[latest.columnKey];
    undoMutation.mutate({
      entry: latest,
      context: {
        userId,
        operation: "undo",
        row: latest.previousRow,
        bookTitle: latest.previousRow.title,
        columnKey: latest.columnKey,
        pipeline: latest.pipeline,
        workScoped: route.pipeline === "metadata" && route.workScoped,
        value: valueForRow(latest.previousRow, latest.columnKey),
        display: displayValue(columns, latest.columnKey, latest.previousRow),
        listKey: latest.listKey,
      },
    });
  }

  function onGridKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key !== "z" || !(event.ctrlKey || event.metaKey) || isEditableTarget(event.target))
      return;
    event.preventDefault();
    undo();
  }

  return {
    pendingCells,
    onCellEdit,
    onGridKeyDown,
    undo,
    undoAvailable,
    undoPending: undoMutation.isPending,
    undoLabel:
      latest === undefined
        ? undefined
        : `${COLUMN_LABELS[latest.columnKey]} for “${latest.previousRow.title}”`,
  };
}
