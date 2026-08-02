/**
 * Cell-edit orchestration for the library table.
 *
 * `EDIT_ROUTES` describes how a column writes; this hook is the plumbing
 * that runs on top of it, including deriving each edit's undo strategy:
 * firing the right mutation for a committed cell edit, patching the
 * TanStack Query list cache from the server's applied value (never the
 * local draft, since the server normalizes input, e.g. ISBN checksums),
 * tracking in-flight cells so the table can render a draft overlay, and a
 * bounded undo stack (toast action plus Ctrl+Z). The query cache stays
 * server-confirmed throughout: there is no optimistic commit.
 *
 * `listKey` must be the exact key the library page's suspense query reads
 * (cursor stripped). The caller owns computing that key; this hook only
 * consumes it. A mismatched key silently writes a dead cache entry.
 */
import {
  useMutation,
  useQueryClient,
  type InfiniteData,
  type QueryClient,
} from "@tanstack/react-query";
import { useRef, useState, type KeyboardEvent } from "react";
import { toast } from "sonner";

import {
  ApiError,
  revertField,
  updateBookMetadata,
  type BookListItem,
  type BookListResponse,
  type UpdateBookMetadataFields,
} from "@/api";
import type { FieldVersionChange } from "@/api/books";
import { updateReadingState, type ReadingState, type UpdateReadingFields } from "@/api/reading";
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

type UndoEntry = MetadataUndoEntry | ReadingUndoEntry;

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
    void queryClient.invalidateQueries({
      queryKey: queryKeys.books.detailsAll,
    });
    return;
  }
  void queryClient.invalidateQueries({
    queryKey: queryKeys.books.detail(manifestationId),
  });
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
    (item) =>
      route.applyToRow(item, {
        value,
        version_id: null,
        previous_version_id: null,
      }),
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

export type UseCellEditOptions = {
  /** Exact key of the suspense list query this hook patches (cursor stripped). */
  listKey: BooksListKey;
  /** Column defs, consulted only for their `accessor` to stage a pending display value. */
  columns: readonly GridColumn<BookListItem>[];
};

export type UseCellEditResult = {
  /** `${row.id}:${columnKey}` -> the draft display value while a commit is in flight. */
  pendingCells: ReadonlyMap<string, string>;
  onCellEdit: (report: CellEditReport<BookListItem>) => void;
  /** Attach to the table's own wrapper element (never `window`) so Ctrl+Z only fires with grid focus. */
  onGridKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
};

export function useCellEdit({ listKey, columns }: UseCellEditOptions): UseCellEditResult {
  const queryClient = useQueryClient();
  const [pendingCells, setPendingCells] = useState<ReadonlyMap<string, string>>(new Map());
  const undoStack = useRef<UndoEntry[]>([]);
  const undoIdCounter = useRef(0);

  const metadataMutation = useMutation({
    mutationFn: ({
      manifestationId,
      patch,
    }: {
      manifestationId: string;
      patch: UpdateBookMetadataFields;
    }) => updateBookMetadata(manifestationId, patch),
  });
  const readingMutation = useMutation({
    mutationFn: ({
      manifestationId,
      patch,
    }: {
      manifestationId: string;
      patch: UpdateReadingFields;
    }) => updateReadingState(manifestationId, patch),
  });

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
    const stack = undoStack.current;
    stack.push(entry);
    // Bounded LIFO: the oldest entry falls off once the cap is exceeded.
    if (stack.length > UNDO_STACK_CAP) stack.shift();
  }

  function removeUndoEntry(id: string): void {
    const stack = undoStack.current;
    const index = stack.findIndex((entry) => entry.id === id);
    if (index !== -1) stack.splice(index, 1);
  }

  // Declared before `announceSuccess` (rather than relying on function
  // hoisting) so the toast action's closure never forward-references it.
  async function handleUndo(entry: UndoEntry): Promise<void> {
    // Removed wherever it sits in the stack: the toast action undoes its
    // own entry even when a later edit has since become the LIFO top.
    removeUndoEntry(entry.id);
    try {
      if (entry.pipeline === "metadata") {
        if (entry.strategy === "revert") {
          await revertField(entry.manifestationId, entry.fieldName, entry.previousVersionId);
        } else {
          await metadataMutation.mutateAsync({
            manifestationId: entry.manifestationId,
            patch: entry.counterPatch,
          });
        }
        restoreMetadataRow(queryClient, listKey, entry);
        // Mirrors handleCellEdit's own invalidation, including the sibling
        // fan-out for a work-scoped field: without it, a book-detail view
        // open on any restored sibling can keep serving the undone value
        // for the remainder of its staleTime.
        const route = EDIT_ROUTES[entry.columnKey];
        invalidateAffectedDetails(
          queryClient,
          entry.manifestationId,
          route.pipeline === "metadata" && route.workScoped,
        );
      } else {
        const response = await readingMutation.mutateAsync({
          manifestationId: entry.manifestationId,
          patch: entry.counterPatch,
        });
        restoreReadingRow(queryClient, listKey, entry, response);
        // The reading pipeline never fans out across sibling editions.
        void queryClient.invalidateQueries({
          queryKey: queryKeys.books.detail(entry.manifestationId),
        });
      }
      toast.success("Reverted.", { id: entry.id });
    } catch (err) {
      console.error("[useCellEdit.handleUndo] undo failed", err);
      toast.error(formatError(err));
    }
  }

  function announceSuccess(columnKey: EditableColumnKey, entry: UndoEntry | null): void {
    const label = COLUMN_LABELS[columnKey];
    if (entry === null) {
      toast.success(`${label} updated.`);
      return;
    }
    pushUndo(entry);
    toast.success(`${label} updated.`, {
      id: entry.id,
      action: {
        label: "Undo",
        onClick: () => {
          void handleUndo(entry);
        },
      },
    });
  }

  async function handleCellEdit(report: CellEditReport<BookListItem>): Promise<void> {
    const { row, previousRow, columnKey } = report;
    if (!isEditableColumnKey(columnKey)) return;
    const route = EDIT_ROUTES[columnKey];

    if (route.pipeline === "metadata") {
      if (!isMetadataColumnKey(columnKey)) return;
      const patch = buildPatchIfChanged(route, previousRow, row);
      if (patch === null) return;

      const key = pendingKey(row.id, columnKey);
      setPending(key, displayValue(columns, columnKey, row));
      try {
        const response = await metadataMutation.mutateAsync({
          manifestationId: row.id,
          patch,
        });
        // Invariant: a successful metadata PATCH always echoes every
        // touched field back in `fields`, keyed by the same field name.
        const applied = lookupAppliedField(response.fields, route.field);
        if (applied === undefined) {
          // Distinct from the mutation-failure branch below: the server
          // accepted the write (2xx) but its response didn't carry the
          // field this route needs, so there's nothing safe to fold into
          // the cache.
          console.error(
            "[useCellEdit.onCellEdit] metadata response missing echoed field",
            route.field,
          );
          clearPending(key);
          toast.error("The server response was missing the edited field.");
          return;
        }
        updateListCache(
          queryClient,
          listKey,
          (item) => affectsManifestation(item, row.id, row.work_id, route.workScoped),
          (item) => route.applyToRow(item, applied),
        );
        invalidateAffectedDetails(queryClient, row.id, route.workScoped);
        clearPending(key);
        announceSuccess(
          columnKey,
          buildMetadataUndoEntry(nextUndoId(), columnKey, route, row, previousRow, applied),
        );
      } catch (err) {
        clearPending(key);
        console.error("[useCellEdit.onCellEdit] metadata mutation failed", err);
        toast.error(formatError(err));
      }
      return;
    }

    if (!isReadingColumnKey(columnKey)) return;
    const patch = buildPatchIfChanged(route, previousRow, row);
    if (patch === null) return;

    const key = pendingKey(row.id, columnKey);
    setPending(key, displayValue(columns, columnKey, row));
    try {
      const response = await readingMutation.mutateAsync({
        manifestationId: row.id,
        patch,
      });
      updateListCache(
        queryClient,
        listKey,
        (item) => item.id === row.id,
        (item) => route.applyToRow(item, response),
      );
      clearPending(key);
      announceSuccess(columnKey, {
        id: nextUndoId(),
        pipeline: "reading",
        manifestationId: row.id,
        columnKey,
        counterPatch: route.toPatch(row, previousRow),
        previousRow,
      });
    } catch (err) {
      clearPending(key);
      console.error("[useCellEdit.onCellEdit] reading mutation failed", err);
      toast.error(formatError(err));
    }
  }

  function onCellEdit(report: CellEditReport<BookListItem>): void {
    void handleCellEdit(report);
  }

  function onGridKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key !== "z" || !(event.ctrlKey || event.metaKey)) return;
    // An open editor gets the keystroke instead, so its own (native) undo
    // isn't shadowed by the grid-level stack.
    if (isEditableTarget(event.target)) return;
    event.preventDefault();
    const entry = undoStack.current.pop();
    if (entry === undefined) return;
    void handleUndo(entry);
  }

  return { pendingCells, onCellEdit, onGridKeyDown };
}
