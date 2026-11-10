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
import { useEffect, useState } from "react";
import { useKeyboard } from "react-aria";
import type { AuthMe } from "@/hooks/useAuthMe";

import {
  ApiError,
  isIfMatchMismatch,
  isIfMatchRequired,
  resolveMetadataOperation,
  undoMetadataOperation,
  type MetadataSnapshot,
  type UndoOperation,
  type OperationResponse,
  updateBookMetadata,
  type BookListItem,
  type BookListResponse,
  type UpdateBookMetadataFields,
} from "@/api";
import {
  getReadingState,
  updateReadingState,
  type ReadingSnapshot,
  type UpdateReadingFields,
} from "@/api/reading";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import { EDIT_ROUTES, type EditableColumnKey } from "./edit-routing";

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

type UndoEntry = {
  context: CellEditContext;
  operation: UndoOperation;
  row: BookListItem;
  columnKey: MetadataColumnKey;
  listKey: BooksListKey;
  etag: string;
};

/** `${row.id}:${columnKey}`, the shared key shape for the pending-cells map. */
export function pendingKey(rowId: string, columnKey: string): string {
  return `${rowId}:${columnKey}`;
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

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable ||
    target.closest('[role="dialog"]') !== null
  );
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
  baseline: string;
  requestId: string;
  undoContext: object;
};

type CellEditRecord = {
  id: number;
  status: "idle" | "pending" | "success" | "error";
  error: Error | null;
  paused: boolean;
  context: CellEditContext | undefined;
};

function cellEditKey(userId: string | null) {
  return ["library-cell-edit", userId] as const;
}

function cellMutations(client: QueryClient, userId: string | null) {
  // This account-scoped key belongs only to the mutations carrying CellEditContext.
  return client.getMutationCache().findAll({
    mutationKey: cellEditKey(userId),
    exact: true,
  }) as Mutation<unknown, Error, { context: CellEditContext }>[];
}

function sameCell(first: CellEditContext | undefined, second: CellEditContext): boolean {
  return first?.row.id === second.row.id && first.columnKey === second.columnKey;
}

function retireCellFailures(
  client: QueryClient,
  context: CellEditContext,
  outcome: "success" | "error" = "success",
): void {
  const cache = client.getMutationCache();
  const mutations = cellMutations(client, context.userId);
  const current = mutations.find((mutation) => mutation.state.variables?.context === context);
  if (current === undefined) return;
  if (outcome === "error") {
    if (
      mutations.some(
        (mutation) =>
          mutation.mutationId > current.mutationId &&
          mutation.state.status === "success" &&
          sameCell(mutation.state.variables?.context, context),
      )
    )
      cache.remove(current);
    return;
  }
  for (const mutation of mutations) {
    if (
      mutation.mutationId < current.mutationId &&
      mutation.state.status === "error" &&
      sameCell(mutation.state.variables?.context, context)
    )
      cache.remove(mutation);
  }
}

function resourceKey(context: CellEditContext): QueryKey {
  return context.pipeline === "metadata"
    ? queryKeys.books.metadata(context.row.id)
    : queryKeys.books.reading(context.row.id, context.userId);
}

function affectsResource(write: CellEditContext, read: CellEditContext): boolean {
  return (
    write.pipeline === read.pipeline &&
    (write.row.id === read.row.id || (write.workScoped && write.row.work_id === read.row.work_id))
  );
}

function hasPendingResourceWrite(client: QueryClient, context: CellEditContext): boolean {
  return cellMutations(client, context.userId).some((mutation) => {
    const write = mutation.state.variables?.context;
    return (
      mutation.state.status === "pending" && write !== undefined && affectsResource(write, context)
    );
  });
}

async function cancelRecoveryReads(client: QueryClient, context: CellEditContext): Promise<void> {
  const affected = cellMutations(client, context.userId).flatMap((mutation) => {
    const read = mutation.state.variables?.context;
    return read !== undefined && affectsResource(context, read) ? [read] : [];
  });
  await Promise.all(
    [context, ...affected].map((read) =>
      client.cancelQueries({ queryKey: [...resourceKey(read), "cell-recovery"] }),
    ),
  );
}

function valueForRow(row: BookListItem, columnKey: EditableColumnKey): unknown {
  return isReadingColumnKey(columnKey) ? (row.reading_state?.[columnKey] ?? null) : row[columnKey];
}

function readValue(context: CellEditContext, data: MetadataSnapshot | ReadingSnapshot): unknown {
  if ("contributors" in data.values && isMetadataColumnKey(context.columnKey)) {
    return context.columnKey === "authors"
      ? data.values.contributors.author
      : data.values[context.columnKey];
  }
  if ("status" in data.values && isReadingColumnKey(context.columnKey))
    return data.values[context.columnKey];
  return undefined;
}

function matchesRead(context: CellEditContext, data: MetadataSnapshot | ReadingSnapshot): boolean {
  const value = readValue(context, data);
  const submitted = context.value;
  if (context.columnKey === "isbn_13" && typeof value === "string" && typeof submitted === "string")
    return value === submitted.replace(/[ -]/g, "");
  return JSON.stringify(value) === JSON.stringify(submitted);
}

export function reconcileMetadata(
  client: QueryClient,
  row: BookListItem,
  listKey: BooksListKey,
  snapshot: MetadataSnapshot,
): void {
  const { values, etag } = snapshot;
  client.setQueryData(queryKeys.books.metadata(row.id), snapshot);
  updateListCache(
    client,
    listKey,
    (item) => item.id === row.id,
    (item) => ({
      ...item,
      title: values.title,
      subtitle: values.subtitle,
      authors: values.contributors.author,
      contributors: [
        ...values.contributors.editor.map((name) => ({ name, role: "editor" as const })),
        ...values.contributors.translator.map((name) => ({ name, role: "translator" as const })),
        ...item.contributors.filter((contributor) => contributor.role === "narrator"),
      ],
      isbn_13: values.isbn_13,
      pages: values.pages,
      genres: values.genres,
      moods: values.moods,
      tags: values.tags,
      content_rating: values.content_rating,
      metadata_etag: etag,
    }),
  );
  void client.invalidateQueries({ queryKey: ["books", "list"] });
  void client.invalidateQueries({
    queryKey: ["books", "metadata"],
    predicate: (query) => query.queryKey.length === 3 && query.queryKey[2] !== row.id,
  });
  void client.invalidateQueries({ queryKey: queryKeys.books.detailsAll });
}

function applyRecoveryRead(
  client: QueryClient,
  context: CellEditContext,
  data: MetadataSnapshot | ReadingSnapshot,
): void {
  client.setQueryData(resourceKey(context), data);
  if ("contributors" in data.values) {
    reconcileMetadata(client, context.row, context.listKey, {
      values: data.values,
      etag: data.etag,
    });
  } else {
    const values = data.values;
    updateListCache(
      client,
      context.listKey,
      (row) => row.id === context.row.id,
      (row) => ({ ...row, reading_state: values, reading_etag: data.etag }),
    );
    void client.invalidateQueries({ queryKey: ["books", "list"] });
    invalidateAffectedDetails(client, context.row.id, false);
  }
}

function useCellEditRecords(): CellEditRecord[] {
  const client = useQueryClient();
  const userId = client.getQueryData<AuthMe>(queryKeys.auth.me())?.id ?? null;
  return useMutationState<CellEditRecord, Mutation<unknown, Error, { context: CellEditContext }>>({
    filters: { mutationKey: cellEditKey(userId) },
    select: (mutation) => ({
      id: mutation.mutationId,
      status: mutation.state.status,
      error: mutation.state.error,
      paused: mutation.state.isPaused,
      context: mutation.state.variables?.context,
    }),
  });
}

type CellEditFeedback = {
  id: number;
  rowId: string;
  columnKey: string;
  bookTitle: string;
  field: string;
  submitted: string;
  message: string;
  reloading: boolean;
  reload: () => void;
};

export type CellEditReport = { row: BookListItem; previousRow: BookListItem; columnKey: string };

export type UseCellEditResult = {
  pendingCells: ReadonlyMap<string, string>;
  onCellEdit: (report: CellEditReport) => void;
  keyboardProps: ReturnType<typeof useKeyboard>["keyboardProps"];
  undo: () => void;
  undoPending: boolean;
  undoAvailable: boolean;
  undoLabel: string | undefined;
  feedback: CellEditFeedback[];
  undoOutcome: string | null;
};

export function useCellEdit({
  listKey,
  enabled,
}: {
  listKey: BooksListKey;
  enabled: boolean;
}): UseCellEditResult {
  const client = useQueryClient();
  const userId = client.getQueryData<AuthMe>(queryKeys.auth.me())?.id ?? null;
  const [owner, setOwner] = useState({ enabled, context: {}, entries: [] as UndoEntry[] });
  const [undoOutcome, setUndoOutcome] = useState<string | null>(null);
  if (enabled !== owner.enabled) {
    setOwner({ enabled, context: {}, entries: [] });
    setUndoOutcome(null);
  }
  const contextOwner = owner.context;
  const [pendingCells, setPendingCells] = useState<ReadonlyMap<string, string>>(new Map());
  const records = useCellEditRecords();

  function drop(operationId: string): void {
    setOwner((current) => ({
      ...current,
      entries: current.entries.filter((entry) => entry.operation.operation_id !== operationId),
    }));
  }

  const deadline = owner.entries.reduce(
    (next, entry) => Math.min(next, Date.parse(entry.operation.expires_at)),
    Infinity,
  );
  useEffect(() => {
    if (!Number.isFinite(deadline)) return;
    const timer = setTimeout(
      () => {
        setOwner((current) => ({
          ...current,
          entries: current.entries.filter(
            (entry) => Date.parse(entry.operation.expires_at) > Date.now(),
          ),
        }));
      },
      Math.max(0, deadline - Date.now()),
    );
    return () => {
      clearTimeout(timer);
    };
  }, [deadline]);

  function adopt(
    context: CellEditContext,
    response: OperationResponse | (MetadataSnapshot & { operations: UndoOperation[] }),
  ): void {
    reconcileMetadata(client, context.row, context.listKey, response);
    const route = EDIT_ROUTES[context.columnKey];
    if (route.pipeline !== "metadata" || !isMetadataColumnKey(context.columnKey)) return;
    const columnKey = context.columnKey;
    const operation = response.operations.find((item) => item.field_name === route.field);
    setOwner((current) => {
      if (!current.enabled || current.context !== context.undoContext) return current;
      if (
        response.operations.length > 0 &&
        (operation === undefined ||
          operation.status === "unavailable" ||
          operation.status === "consumed")
      )
        return current;
      const entries = current.entries.filter(
        (entry) =>
          !(
            entry.operation.field_name === route.field &&
            (route.workScoped
              ? entry.row.work_id === context.row.work_id
              : entry.row.id === context.row.id)
          ),
      );
      if (
        operation !== undefined &&
        (operation.status === "eligible" || operation.status === "locked") &&
        Date.parse(operation.expires_at) > Date.now()
      ) {
        entries.push({
          context,
          operation,
          row: context.row,
          columnKey,
          listKey: context.listKey,
          etag: response.etag,
        });
      }
      return { ...current, entries: entries.slice(-UNDO_STACK_CAP) };
    });
    retireCellFailures(client, context);
  }

  function failed(_error: Error, { context }: { context: CellEditContext }): void {
    retireCellFailures(client, context, "error");
  }
  function settled(
    _data: unknown,
    _error: unknown,
    { context }: { context: CellEditContext },
  ): void {
    setPendingCells((previous) => {
      const next = new Map(previous);
      next.delete(pendingKey(context.row.id, context.columnKey));
      return next;
    });
  }
  const metadataMutation = useMutation({
    mutationKey: cellEditKey(userId),
    onMutate: ({ context }) => cancelRecoveryReads(client, context),
    mutationFn: ({
      context,
      patch,
    }: {
      context: CellEditContext;
      patch: UpdateBookMetadataFields;
    }) => updateBookMetadata(context.row.id, patch, context.baseline, context.requestId),
    onSuccess: (response, { context }) => {
      adopt(context, response);
    },
    onError: failed,
    onSettled: settled,
  });
  const readingMutation = useMutation({
    mutationKey: cellEditKey(userId),
    onMutate: ({ context }) => cancelRecoveryReads(client, context),
    mutationFn: ({ context, patch }: { context: CellEditContext; patch: UpdateReadingFields }) =>
      updateReadingState(context.row.id, patch, context.baseline),
    onSuccess: (response, { context }) => {
      applyRecoveryRead(client, context, response);
      retireCellFailures(client, context);
    },
    onError: failed,
    onSettled: settled,
  });

  const undoMutation = useMutation({
    mutationKey: cellEditKey(userId),
    onMutate: ({ context }) => cancelRecoveryReads(client, context),
    mutationFn: async ({ entry }: { entry: UndoEntry; context: CellEditContext }) => {
      try {
        return await undoMetadataOperation(entry.row.id, entry.operation.operation_id, entry.etag);
      } catch (error) {
        if (!isIfMatchMismatch(error)) throw error;
        const resolution = await client.query({
          queryKey: [
            ...queryKeys.books.metadata(entry.row.id),
            "undo-recovery",
            entry.operation.operation_id,
          ],
          queryFn: ({ signal }) =>
            resolveMetadataOperation(
              entry.row.id,
              { operation_id: entry.operation.operation_id },
              signal,
            ),
          staleTime: 0,
        });
        reconcileMetadata(client, entry.row, entry.listKey, resolution);
        if (resolution.outcome === "restored") return resolution;
        const operation = resolution.operations.find(
          (item) => item.operation_id === entry.operation.operation_id,
        );
        if (operation?.status === "unavailable" || operation?.status === "consumed") {
          throw new ApiError(
            422,
            "/undo-unavailable",
            "Undo unavailable",
            "This saved edit can no longer be undone.",
          );
        }
        if (operation?.status === "locked")
          throw new ApiError(
            409,
            "/field-locked",
            "Field locked",
            "Unlock this field before retrying Undo.",
          );
        if (operation?.status !== "eligible") throw error;
        return undoMetadataOperation(entry.row.id, entry.operation.operation_id, resolution.etag);
      }
    },
    onSuccess: (response, { entry, context }) => {
      retireCellFailures(client, context);
      reconcileMetadata(client, entry.row, entry.listKey, response);
      drop(entry.operation.operation_id);
      setUndoOutcome(`${COLUMN_LABELS[entry.columnKey]} for “${entry.row.title}” restored.`);
    },
    onError: (error, { entry }) => {
      if (
        error instanceof ApiError &&
        (error.problemSlug === "undo-unavailable" || error.problemSlug === "undo-conflict")
      )
        drop(entry.operation.operation_id);
      setUndoOutcome(
        `Undo ${COLUMN_LABELS[entry.columnKey]} for “${entry.row.title}”: ${formatError(error)}`,
      );
    },
  });

  const errors = records.flatMap((record) => {
    const context = record.context;
    if (record.status !== "error" || context === undefined || context.operation !== "save")
      return [];
    return [
      {
        ...record,
        context,
        blocked: records.some(
          (write) =>
            write.status === "pending" &&
            write.context !== undefined &&
            affectsResource(write.context, context),
        ),
      },
    ];
  });
  const recovery = useQueries({
    queries: errors.map((record) =>
      queryOptions({
        queryKey: [...resourceKey(record.context), "cell-recovery", record.id],
        queryFn: async ({ signal }) => {
          const context = record.context;
          if (context.pipeline === "metadata") {
            const response = await resolveMetadataOperation(
              context.row.id,
              { request_id: context.requestId },
              signal,
            );
            signal.throwIfAborted();
            applyRecoveryRead(client, context, response);
            if (response.outcome === "saved") adopt(context, response);
            return { saved: response.outcome === "saved" };
          }
          const response = await getReadingState(context.row.id, signal);
          signal.throwIfAborted();
          applyRecoveryRead(client, context, response);
          return { saved: matchesRead(context, response) };
        },
        enabled: record.blocked ? false : (query) => query.state.data?.saved !== true,
        staleTime: 0,
        refetchOnWindowFocus: false,
      }),
    ),
  });
  const feedback = errors.flatMap((record, index) => {
    const read = recovery[index];
    if (read.isSuccess && read.data.saved) return [];
    const context = record.context;
    let message = formatError(record.error);
    if (isIfMatchMismatch(record.error) || isIfMatchRequired(record.error))
      message = "This book changed elsewhere. Reload the latest values before editing again.";
    else if (
      context.columnKey === "isbn_13" &&
      record.error instanceof ApiError &&
      record.error.status === 422 &&
      record.error.errors?.isbn_13 !== undefined
    )
      message = "ISBN-13 wasn’t saved. Check the 13-digit number; spaces and hyphens are allowed.";
    if (read.isError) message += " Connection problem. Your input is retained. Try Reload again.";
    return [
      {
        id: record.id,
        rowId: context.row.id,
        columnKey: context.columnKey,
        bookTitle: context.bookTitle,
        field: COLUMN_LABELS[context.columnKey],
        submitted: context.display,
        message,
        reloading: read.isFetching || record.blocked,
        reload: () => {
          if (!hasPendingResourceWrite(client, context)) void read.refetch();
        },
      },
    ];
  });

  function onCellEdit({ row, previousRow, columnKey }: CellEditReport): void {
    if (!enabled || !isEditableColumnKey(columnKey)) return;
    setUndoOutcome(null);
    const route = EDIT_ROUTES[columnKey];
    const value = valueForRow(row, columnKey);
    let display = JSON.stringify(value);
    if (value === null) display = "unset";
    else if (typeof value === "string") display = value;
    const context: CellEditContext = {
      userId,
      operation: "save",
      row: previousRow,
      bookTitle: previousRow.title,
      columnKey,
      pipeline: route.pipeline,
      workScoped: route.pipeline === "metadata" && route.workScoped,
      value,
      display,
      listKey,
      baseline:
        route.pipeline === "metadata" ? previousRow.metadata_etag : previousRow.reading_etag,
      requestId: crypto.randomUUID(),
      undoContext: contextOwner,
    };
    if (route.pipeline === "metadata") {
      const patch = buildPatchIfChanged(route, previousRow, row);
      if (patch !== null) {
        setPendingCells((current) =>
          new Map(current).set(pendingKey(row.id, columnKey), context.display),
        );
        metadataMutation.mutate({ context, patch });
      }
    } else {
      const patch = buildPatchIfChanged(route, previousRow, row);
      if (patch !== null) {
        setPendingCells((current) =>
          new Map(current).set(pendingKey(row.id, columnKey), context.display),
        );
        readingMutation.mutate({ context, patch });
      }
    }
  }
  feedback.push(
    ...records.flatMap((record) =>
      record.status === "pending" &&
      record.paused &&
      record.context !== undefined &&
      record.context.operation === "save"
        ? [
            {
              id: record.id,
              rowId: record.context.row.id,
              columnKey: record.context.columnKey,
              bookTitle: record.context.bookTitle,
              field: COLUMN_LABELS[record.context.columnKey],
              submitted: record.context.display,
              message: "Connection problem. Your input is retained.",
              reloading: true,
              reload: () => {},
            },
          ]
        : [],
    ),
  );
  const latest = undoMutation.isPending ? undoMutation.variables.entry : owner.entries.at(-1);
  const undoAvailable =
    enabled && latest !== undefined && pendingCells.size === 0 && !undoMutation.isPending;
  function undo(): void {
    if (undoAvailable)
      undoMutation.mutate({ entry: latest, context: { ...latest.context, operation: "undo" } });
  }
  const { keyboardProps } = useKeyboard({
    onKeyDown: (event) => {
      if (
        event.key.toLowerCase() !== "z" ||
        event.shiftKey ||
        event.altKey ||
        !(event.ctrlKey || event.metaKey) ||
        isEditableTarget(event.target)
      ) {
        event.continuePropagation();
        return;
      }
      event.preventDefault();
      undo();
    },
  });
  return {
    pendingCells,
    onCellEdit,
    keyboardProps,
    undo,
    undoAvailable,
    undoPending: undoMutation.isPending,
    undoLabel:
      latest === undefined
        ? undefined
        : `${COLUMN_LABELS[latest.columnKey]} for “${latest.row.title}”`,
    feedback,
    undoOutcome,
  };
}
