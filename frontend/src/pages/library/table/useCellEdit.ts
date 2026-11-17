import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useKeyboard } from "react-aria";
import { useRelayEnvironment } from "react-relay";
import { commitLocalUpdate } from "relay-runtime";

import { ReadingStatusSchema, type ReadingStatus } from "@/api/books";
import { ApiError } from "@/api/errors";
import {
  GraphQLExecutionError,
  editingFailureAction,
  encodeGraphQLId,
  executeEditingOnce,
  loadAuthorField,
  type ReaderBook,
} from "@/api/graphql";
import type { LibrarySource } from "@/lib/relay/library-visit";
import {
  mutationsCellsQuery,
  mutationsReadingMutation,
  mutationsReadingQuery,
  mutationsSaveCellsMutation,
  mutationsSaveAuthorsMutation,
  mutationsResolveCellsQuery,
  mutationsResolveAuthorsQuery,
  mutationsUndoCellsMutation,
  mutationsUndoAuthorsMutation,
} from "@/lib/relay/mutations";
import type { mutationsCellsQuery as CellsQuery } from "@/lib/relay/__generated__/mutationsCellsQuery.graphql";
import type {
  mutationsReadingMutation as ReadingMutation,
  ReadingPatch,
} from "@/lib/relay/__generated__/mutationsReadingMutation.graphql";
import type { mutationsReadingQuery as ReadingQuery } from "@/lib/relay/__generated__/mutationsReadingQuery.graphql";
import type {
  mutationsSaveCellsMutation as SaveCells,
  MetadataPatch,
} from "@/lib/relay/__generated__/mutationsSaveCellsMutation.graphql";
import type { mutationsSaveAuthorsMutation as SaveAuthors } from "@/lib/relay/__generated__/mutationsSaveAuthorsMutation.graphql";
import type { mutationsResolveCellsQuery as ResolveCells } from "@/lib/relay/__generated__/mutationsResolveCellsQuery.graphql";
import type { mutationsResolveAuthorsQuery as ResolveAuthors } from "@/lib/relay/__generated__/mutationsResolveAuthorsQuery.graphql";
import type { mutationsUndoCellsMutation as UndoCells } from "@/lib/relay/__generated__/mutationsUndoCellsMutation.graphql";
import type { mutationsUndoAuthorsMutation as UndoAuthors } from "@/lib/relay/__generated__/mutationsUndoAuthorsMutation.graphql";
import { EDIT_ROUTES, READING_STATUS_INPUT, type EditableColumnKey } from "./edit-routing";

const COLUMN_LABELS: Record<EditableColumnKey, string> = {
  title: "Title",
  subtitle: "Subtitle",
  isbn_13: "ISBN",
  pages: "Pages",
  authors: "Authors",
  status: "Status",
  rating: "Rating",
};
const UNDO_STACK_CAP = 10;
type CellsSnapshot = SaveCells["response"]["updateMetadata"]["metadata"];
type AuthorSnapshot = SaveAuthors["response"]["updateMetadata"]["metadata"];
type MetadataResult = {
  metadata: CellsSnapshot | AuthorSnapshot;
  operations: SaveCells["response"]["updateMetadata"]["operations"];
};
type ReadingSnapshot = ReadingMutation["response"]["updateReadingState"];
type RecoveryAction = "retry" | "reload" | "correct" | "none";
type Attempt = {
  id: string;
  row: ReaderBook;
  draft: ReaderBook;
  columnKey: EditableColumnKey;
  value: unknown;
  display: string;
  baseline: string;
  source: LibrarySource;
  release: () => void;
  undoContext: object;
} & ({ pipeline: "metadata"; patch: MetadataPatch } | { pipeline: "reading"; patch: ReadingPatch });
type EditRecord = {
  attempt: Attempt;
  pending: boolean;
  retrying: boolean;
  reloading: boolean;
  error: Error | null;
  action: RecoveryAction;
  reset: ReaderBook | undefined;
};
type UndoEntry = {
  attempt: Attempt;
  operation: MetadataResult["operations"][number];
  baseline: string;
  release: () => void;
};
type Owner = { enabled: boolean; context: object; entries: UndoEntry[] };

/** Identify one editable cell across pending, failure and saved feedback. */
export function pendingKey(rowId: string, columnKey: string): string {
  return `${rowId}:${columnKey}`;
}

function formatError(error: unknown): string {
  if (error instanceof ApiError)
    return [error.title, error.detail].filter(Boolean).join(": ") || "Request failed.";
  return error instanceof Error ? error.message : "Request failed.";
}

function errorCode(error: unknown): string | undefined {
  if (error instanceof GraphQLExecutionError) return error.errors.at(0)?.extensions?.code;
  if (error instanceof ApiError) return error.problemSlug ?? undefined;
  return undefined;
}

function displayValue(value: string | number | string[] | null): string {
  if (Array.isArray(value)) return value.join(", ");
  return value === null ? "unset" : String(value);
}

function isEditableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.matches("input, textarea, select") ||
      target.isContentEditable ||
      target.closest('[role="dialog"]') !== null)
  );
}

type CellEditFeedback = {
  id: string;
  rowId: string;
  columnKey: EditableColumnKey;
  bookTitle: string;
  field: string;
  submitted: string;
  message: string;
  action: RecoveryAction;
  reloading: boolean;
  retrying: boolean;
  retained: ReaderBook | undefined;
  retainSource: () => () => void;
  reload: () => void;
  retry: () => void;
};
export type CellEditReport = { row: ReaderBook; previousRow: ReaderBook; columnKey: string };
export type StatusTarget = {
  id: string;
  attemptId: string;
  send: () => Promise<{ saved: boolean; action: RecoveryAction } | undefined>;
};
export type UseCellEditResult = {
  pendingCells: ReadonlyMap<string, string>;
  savedCells: ReadonlySet<string>;
  onCellEdit: (report: CellEditReport) => void;
  retainSource: () => () => void;
  statusTargets: (rows: readonly ReaderBook[], status: ReadingStatus | null) => StatusTarget[];
  keyboardProps: ReturnType<typeof useKeyboard>["keyboardProps"];
  undo: () => void;
  undoPending: boolean;
  undoAvailable: boolean;
  undoLabel: string | undefined;
  feedback: CellEditFeedback[];
  undoOutcome: string | null;
};

/** Own retained cell attempts, authoritative recovery and short-lived exact Undo. */
export function useCellEdit({
  source,
  enabled,
  onChanged,
}: {
  source: LibrarySource | undefined;
  enabled: boolean;
  onChanged: () => void;
}): UseCellEditResult {
  const environment = useRelayEnvironment();
  const controller = useRef<AbortController | null>(null);
  const [records, setRecords] = useState<EditRecord[]>([]);
  const [savedCells, setSavedCells] = useState<ReadonlySet<string>>(new Set());
  const [owner, setOwner] = useState<Owner>({ enabled, context: {}, entries: [] });
  const [undoPending, setUndoPending] = useState(false);
  const [undoOutcome, setUndoOutcome] = useState<string | null>(null);
  if (enabled !== owner.enabled) {
    setOwner({ enabled, context: {}, entries: [] });
    setUndoOutcome(null);
  }
  const latestOwner = useRef(owner);
  const held = useRef(new Set<() => void>());
  const committedUndo = useRef<UndoEntry[]>([]);
  const adopted = useRef(new Set<string>());
  useLayoutEffect(() => {
    latestOwner.current = owner;
  }, [owner]);
  useEffect(() => {
    for (const entry of committedUndo.current) {
      if (
        !owner.entries.some(
          (current) => current.operation.operationId === entry.operation.operationId,
        )
      )
        entry.release();
    }
    committedUndo.current = owner.entries;
  }, [owner.entries]);
  useEffect(() => {
    const lifetime = new AbortController();
    const releases = held.current;
    controller.current = lifetime;
    return () => {
      lifetime.abort();
      for (const release of releases) release();
    };
  }, []);
  const deadline = owner.entries.reduce(
    (next, entry) => Math.min(next, Date.parse(entry.operation.expiresAt)),
    Infinity,
  );
  useEffect(() => {
    if (!Number.isFinite(deadline)) return;
    const removeExpired = (current: Owner): Owner => ({
      ...current,
      entries: current.entries.filter(
        (entry) => Date.parse(entry.operation.expiresAt) > Date.now(),
      ),
    });
    const timer = setTimeout(
      () => {
        setOwner(removeExpired);
      },
      Math.max(0, deadline - Date.now()),
    );
    return () => {
      clearTimeout(timer);
    };
  }, [deadline]);

  function retainSource(target: LibrarySource | undefined = source): () => void {
    if (target === undefined) return () => {};
    const release = target.acquire();
    const dispose = (): void => {
      release();
      held.current.delete(dispose);
    };
    held.current.add(dispose);
    return dispose;
  }

  function publishMetadata(attempt: Attempt, result: MetadataResult): void {
    const snapshot = result.metadata;
    const previous = attempt.source.rows.get(attempt.row.id);
    if ("title" in snapshot && previous !== undefined)
      attempt.source.rows.set(attempt.row.id, { ...previous, metadata: snapshot });
    commitLocalUpdate(environment, (store) => {
      const edition = store.get(snapshot.manifestationId);
      const work = store.get(snapshot.workId);
      if ("title" in snapshot) {
        work?.setValue(snapshot.title, "title");
        work?.setValue(snapshot.subtitle, "subtitle");
        edition?.setValue(snapshot.isbn13, "isbn13");
        edition?.setValue(snapshot.pages, "pages");
        const selected = edition?.getLinkedRecord("metadata");
        selected?.setValue(snapshot.etag, "etag");
        selected?.setValue(snapshot.title, "title");
        selected?.setValue(snapshot.subtitle, "subtitle");
        selected?.setValue(snapshot.isbn13, "isbn13");
        selected?.setValue(snapshot.pages, "pages");
      }
      work?.invalidateRecord();
      edition?.invalidateRecord();
    });
    onChanged();
  }

  function publishReading(attempt: Attempt, snapshot: ReadingSnapshot): void {
    const previous = attempt.source.rows.get(attempt.row.id);
    if (previous !== undefined)
      attempt.source.rows.set(attempt.row.id, { ...previous, readingState: snapshot });
    commitLocalUpdate(environment, (store) => {
      const edition = store.get(encodeGraphQLId("Manifestation", attempt.row.id));
      const reading = edition?.getLinkedRecord("readingState");
      for (const [key, value] of Object.entries(snapshot)) reading?.setValue(value, key);
      edition?.invalidateRecord();
    });
    onChanged();
  }

  function acceptMetadata(attempt: Attempt, result: MetadataResult): void {
    if (adopted.current.has(attempt.id)) return;
    adopted.current.add(attempt.id);
    publishMetadata(attempt, result);
    const route = EDIT_ROUTES[attempt.columnKey];
    if (
      route.pipeline === "metadata" &&
      latestOwner.current.enabled &&
      latestOwner.current.context === attempt.undoContext
    ) {
      const operation = result.operations.find((item) => item.fieldName === route.field);
      let entry: UndoEntry | undefined;
      if (
        operation !== undefined &&
        (operation.status === "eligible" || operation.status === "locked") &&
        Date.parse(operation.expiresAt) > Date.now()
      ) {
        const release = retainSource(attempt.source);
        entry = { attempt, operation, baseline: result.metadata.etag, release };
      }
      setOwner((current) => {
        const entries = current.entries.filter(
          (previous) =>
            !(
              previous.operation.fieldName === route.field &&
              (route.workScoped
                ? previous.attempt.row.work_id === attempt.row.work_id
                : previous.attempt.row.id === attempt.row.id)
            ),
        );
        if (entry !== undefined) entries.push(entry);
        return { ...current, entries: entries.slice(-UNDO_STACK_CAP) };
      });
    }
  }

  async function resolve(attempt: Attempt, operationId?: string) {
    const variables = {
      id: encodeGraphQLId("Manifestation", attempt.row.id),
      ...(operationId === undefined ? { requestId: attempt.id } : { operationId }),
    };
    if (attempt.columnKey === "authors") {
      const response = await executeEditingOnce<ResolveAuthors>(
        environment,
        mutationsResolveAuthorsQuery,
        variables,
        controller.current?.signal,
      );
      return response?.metadataOperation;
    }
    const response = await executeEditingOnce<ResolveCells>(
      environment,
      mutationsResolveCellsQuery,
      variables,
      controller.current?.signal,
    );
    return response?.metadataOperation;
  }

  async function sendMetadata(attempt: Attempt & { pipeline: "metadata" }) {
    const id = encodeGraphQLId("Manifestation", attempt.row.id);
    const variables = {
      id,
      baseline: attempt.baseline,
      patch: attempt.patch,
      requestId: attempt.id,
    };
    if (attempt.columnKey === "authors") {
      const response = await executeEditingOnce<SaveAuthors>(
        environment,
        mutationsSaveAuthorsMutation,
        variables,
        controller.current?.signal,
      );
      return response?.updateMetadata;
    } else {
      const response = await executeEditingOnce<SaveCells>(
        environment,
        mutationsSaveCellsMutation,
        variables,
        controller.current?.signal,
      );
      return response?.updateMetadata;
    }
  }

  async function sendReading(attempt: Attempt & { pipeline: "reading" }) {
    const response = await executeEditingOnce<ReadingMutation>(
      environment,
      mutationsReadingMutation,
      {
        id: encodeGraphQLId("Manifestation", attempt.row.id),
        baseline: attempt.baseline,
        patch: attempt.patch,
      },
      controller.current?.signal,
    );
    return response?.updateReadingState;
  }

  async function recoverMetadata(attempt: Attempt): Promise<boolean> {
    const response = await resolve(attempt);
    if (response?.outcome !== "saved") return false;
    acceptMetadata(attempt, response);
    return true;
  }

  async function recoverReading(attempt: Attempt): Promise<boolean> {
    const response = await executeEditingOnce<ReadingQuery>(
      environment,
      mutationsReadingQuery,
      { id: encodeGraphQLId("Manifestation", attempt.row.id) },
      controller.current?.signal,
    );
    const reading = response?.manifestation?.readingState;
    if (
      reading !== undefined &&
      ((attempt.columnKey === "status" && reading.status === attempt.value) ||
        (attempt.columnKey === "rating" && reading.rating === attempt.value))
    ) {
      publishReading(attempt, reading);
      return true;
    }
    return false;
  }

  async function sendAttempt(attempt: Attempt): Promise<boolean> {
    if (attempt.pipeline === "metadata") {
      const response = await sendMetadata(attempt);
      if (response === undefined) return false;
      acceptMetadata(attempt, response);
    } else {
      const response = await sendReading(attempt);
      if (response === undefined) return false;
      publishReading(attempt, response);
    }
    return true;
  }

  function recoverAttempt(attempt: Attempt): Promise<boolean> {
    return attempt.pipeline === "metadata" ? recoverMetadata(attempt) : recoverReading(attempt);
  }

  async function save(
    attempt: Attempt,
    retrying = false,
  ): Promise<{ saved: boolean; action: RecoveryAction } | undefined> {
    setRecords((current) => {
      const previous = current.find((record) => record.attempt.id === attempt.id);
      return [
        ...current.filter((record) => record.attempt.id !== attempt.id),
        {
          attempt,
          pending: true,
          retrying,
          reloading: false,
          error: previous?.error ?? null,
          action: previous?.action ?? "none",
          reset: undefined,
        },
      ];
    });
    try {
      if (!(await sendAttempt(attempt))) return;
      complete(attempt);
      return { saved: true, action: "none" };
    } catch (error) {
      if (controller.current?.signal.aborted) return;
      const action = editingFailureAction(error);
      if (action === "retry" || action === "reload") {
        try {
          const recovered = await recoverAttempt(attempt);
          if (recovered) {
            complete(attempt);
            return { saved: true, action: "none" };
          }
        } catch (recoveryError) {
          console.error("[useCellEdit] save resolution failed", recoveryError);
        }
      }
      setRecords((current) =>
        current.map((record) =>
          record.attempt.id === attempt.id
            ? {
                ...record,
                pending: false,
                retrying: false,
                action,
                error: error instanceof Error ? error : new Error("Request failed."),
              }
            : record,
        ),
      );
      return { saved: false, action };
    }
  }

  function complete(attempt: Attempt): void {
    attempt.release();
    setRecords((current) => current.filter((record) => record.attempt.id !== attempt.id));
    setSavedCells((current) => new Set(current).add(pendingKey(attempt.row.id, attempt.columnKey)));
  }

  async function reload(record: EditRecord): Promise<void> {
    const attempt = record.attempt;
    setRecords((current) =>
      current.map((item) => (item.attempt.id === attempt.id ? { ...item, reloading: true } : item)),
    );
    try {
      let reset: ReaderBook;
      if (attempt.pipeline === "reading") {
        const response = await executeEditingOnce<ReadingQuery>(
          environment,
          mutationsReadingQuery,
          { id: encodeGraphQLId("Manifestation", attempt.row.id) },
          controller.current?.signal,
        );
        if (response?.manifestation === undefined || response.manifestation === null) return;
        const reading = response.manifestation.readingState;
        publishReading(attempt, reading);
        reset = {
          ...attempt.row,
          reading_etag: reading.etag,
          reading_state: {
            status: ReadingStatusSchema.nullable().parse(reading.status),
            rating: reading.rating ?? null,
            progress_pct: reading.progressPct ?? null,
            started_at: reading.startedAt ?? null,
            finished_at: reading.finishedAt ?? null,
          },
        };
      } else if (attempt.columnKey === "authors") {
        const field = await loadAuthorField(
          environment,
          attempt.row.id,
          controller.current?.signal,
        );
        if (field === undefined) return;
        reset = {
          ...attempt.row,
          authors: field.authors.map((author) => author.name),
          metadata_etag: field.etag,
        };
        onChanged();
      } else {
        const response = await executeEditingOnce<CellsQuery>(
          environment,
          mutationsCellsQuery,
          { id: encodeGraphQLId("Manifestation", attempt.row.id) },
          controller.current?.signal,
        );
        if (response?.manifestation === undefined || response.manifestation === null) return;
        const metadata = response.manifestation.metadata;
        publishMetadata(attempt, { metadata, operations: [] });
        reset = {
          ...attempt.row,
          title: metadata.title,
          subtitle: metadata.subtitle ?? null,
          isbn_13: metadata.isbn13 ?? null,
          pages: metadata.pages ?? null,
          metadata_etag: metadata.etag,
        };
      }
      setRecords((current) =>
        current.map((item) =>
          item.attempt.id === attempt.id ? { ...item, reset, reloading: false } : item,
        ),
      );
    } catch (error) {
      console.error("[useCellEdit] reload failed", error);
      setRecords((current) =>
        current.map((item) =>
          item.attempt.id === attempt.id ? { ...item, reloading: false } : item,
        ),
      );
    }
  }

  function onCellEdit({ row, previousRow, columnKey }: CellEditReport): void {
    if (!enabled || source === undefined || !(columnKey in EDIT_ROUTES)) return;
    const column = Object.keys(COLUMN_LABELS).find(
      (key): key is EditableColumnKey => key === columnKey,
    );
    if (column === undefined) return;
    const route = EDIT_ROUTES[column];
    const previous = records.findLast(
      (record) => record.attempt.row.id === row.id && record.attempt.columnKey === column,
    );
    const opening = previous?.reset ?? previous?.attempt.row ?? previousRow;
    const patch = route.toPatch(opening, row);
    if (
      previous === undefined &&
      JSON.stringify(patch) === JSON.stringify(route.toPatch(opening, opening))
    )
      return;
    const value =
      column === "status" || column === "rating"
        ? (row.reading_state?.[column] ?? null)
        : row[column];
    const display = displayValue(value);
    let baseline = route.pipeline === "metadata" ? opening.metadata_etag : opening.reading_etag;
    if (previous !== undefined && previous.reset === undefined)
      baseline = previous.attempt.baseline;
    let id: string = crypto.randomUUID();
    if (
      previous !== undefined &&
      previous.reset === undefined &&
      JSON.stringify(previous.attempt.patch) === JSON.stringify(patch)
    ) {
      if (previous.action !== "retry") return;
      id = previous.attempt.id;
    }
    const target = previous?.attempt.source ?? source;
    const release = retainSource(target);
    const common = {
      id,
      row: opening,
      draft: {
        ...row,
        metadata_etag: route.pipeline === "metadata" ? baseline : opening.metadata_etag,
        reading_etag: route.pipeline === "reading" ? baseline : opening.reading_etag,
      },
      columnKey: column,
      value,
      display,
      baseline,
      source: target,
      release,
      undoContext: owner.context,
    };
    const attempt: Attempt =
      route.pipeline === "metadata"
        ? { ...common, pipeline: "metadata", patch: route.toPatch(opening, row) }
        : { ...common, pipeline: "reading", patch: route.toPatch(opening, row) };
    previous?.attempt.release();
    setRecords((current) => current.filter((record) => record.attempt.id !== previous?.attempt.id));
    setUndoOutcome(null);
    setSavedCells((current) => {
      const next = new Set(current);
      next.delete(pendingKey(row.id, column));
      return next;
    });
    void save(attempt);
  }

  function statusTargets(
    rows: readonly ReaderBook[],
    status: ReadingStatus | null,
  ): StatusTarget[] {
    if (source === undefined) return [];
    const keepSavedCell = (key: string): boolean =>
      !rows.some((row) => key === pendingKey(row.id, "status"));
    setSavedCells((current) => new Set([...current].filter(keepSavedCell)));
    return rows.map((row) => {
      const previous = records.findLast(
        (record) => record.attempt.row.id === row.id && record.attempt.columnKey === "status",
      );
      const opening = previous?.reset ?? row;
      const attempt: Attempt = {
        id: crypto.randomUUID(),
        row: opening,
        draft: {
          ...opening,
          reading_state: {
            status,
            rating: opening.reading_state?.rating ?? null,
            progress_pct: opening.reading_state?.progress_pct ?? null,
            started_at: opening.reading_state?.started_at ?? null,
            finished_at: opening.reading_state?.finished_at ?? null,
          },
        },
        columnKey: "status",
        value: status,
        display: status ?? "unset",
        baseline: opening.reading_etag,
        source,
        release: retainSource(source),
        undoContext: owner.context,
        pipeline: "reading",
        patch: { status: status === null ? null : READING_STATUS_INPUT[status] },
      };
      previous?.attempt.release();
      const keepRecord = (record: EditRecord): boolean =>
        record.attempt.id !== previous?.attempt.id;
      setRecords((current) => current.filter(keepRecord));
      let sent = false;
      return {
        id: row.id,
        attemptId: attempt.id,
        send: () => {
          const retrying = sent;
          sent = true;
          return save(attempt, retrying);
        },
      };
    });
  }

  const latest = owner.entries.at(-1);
  const undoAvailable =
    enabled && latest !== undefined && !undoPending && !records.some((record) => record.pending);
  async function sendUndo(attempt: Attempt, variables: UndoCells["variables"]) {
    if (attempt.columnKey === "authors")
      return (
        await executeEditingOnce<UndoAuthors>(
          environment,
          mutationsUndoAuthorsMutation,
          variables,
          controller.current?.signal,
        )
      )?.undoMetadata;
    return (
      await executeEditingOnce<UndoCells>(
        environment,
        mutationsUndoCellsMutation,
        variables,
        controller.current?.signal,
      )
    )?.undoMetadata;
  }

  async function recoverUndo(
    entry: UndoEntry,
    variables: UndoCells["variables"],
    response: MetadataResult | undefined,
    failure: unknown,
  ) {
    const attempt = entry.attempt;
    const resolution = await resolve(attempt, entry.operation.operationId);
    if (resolution?.outcome === "restored") response = resolution;
    else {
      const operation = resolution?.operations.find(
        (item) => item.operationId === entry.operation.operationId,
      );
      if (operation?.status === "locked")
        failure = new ApiError(
          409,
          "/field-locked",
          "Field locked",
          "Unlock this field before retrying Undo.",
        );
      else if (operation?.status === "unavailable" || operation?.status === "consumed")
        failure = new ApiError(
          422,
          "/undo-unavailable",
          "Undo unavailable",
          "This saved edit can no longer be undone.",
        );
      else if (resolution !== undefined && operation?.status === "eligible") {
        const retry = { ...variables, baseline: resolution.metadata.etag };
        response = await sendUndo(attempt, retry);
      }
    }
    return { response, failure };
  }

  async function undoEntry(entry: UndoEntry): Promise<void> {
    const attempt = entry.attempt;
    const variables = {
      id: encodeGraphQLId("Manifestation", attempt.row.id),
      operationId: entry.operation.operationId,
      baseline: entry.baseline,
    };
    setUndoPending(true);
    let response: MetadataResult | undefined;
    let failure: unknown;
    try {
      response = await sendUndo(attempt, variables);
    } catch (error) {
      failure = error;
    }
    const action = failure === undefined ? "none" : editingFailureAction(failure);
    if (action === "retry" || action === "reload") {
      try {
        const recovered = await recoverUndo(entry, variables, response, failure);
        response = recovered.response;
        failure = recovered.failure;
      } catch (error) {
        failure = error;
      }
    }
    if (!controller.current?.signal.aborted) {
      if (response !== undefined) {
        publishMetadata(attempt, response);
        setOwner((current) => ({
          ...current,
          entries: current.entries.filter(
            (item) => item.operation.operationId !== entry.operation.operationId,
          ),
        }));
        setUndoOutcome(`${COLUMN_LABELS[attempt.columnKey]} for “${attempt.row.title}” restored.`);
      } else if (failure !== undefined) {
        if (errorCode(failure) === "undo-unavailable" || errorCode(failure) === "undo-conflict")
          setOwner((current) => ({
            ...current,
            entries: current.entries.filter(
              (item) => item.operation.operationId !== entry.operation.operationId,
            ),
          }));
        setUndoOutcome(
          `Undo ${COLUMN_LABELS[attempt.columnKey]} for “${attempt.row.title}”: ${formatError(failure)}`,
        );
      }
    }
    setUndoPending(false);
  }
  const undo = (): void => {
    if (undoAvailable) void undoEntry(latest);
  };
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
  const feedback = records
    .filter((record) => record.error !== null)
    .map((record): CellEditFeedback => ({
      id: record.attempt.id,
      rowId: record.attempt.row.id,
      columnKey: record.attempt.columnKey,
      bookTitle: record.attempt.row.title,
      field: COLUMN_LABELS[record.attempt.columnKey],
      submitted: record.attempt.display,
      message:
        record.action === "reload"
          ? "This book changed elsewhere. Reload the latest values before editing again."
          : formatError(record.error),
      action: record.action,
      reloading: record.reloading,
      retrying: record.retrying,
      retained: record.reset ?? record.attempt.draft,
      retainSource: () => retainSource(record.attempt.source),
      reload: () => {
        if (!records.some((item) => item.pending)) void reload(record);
      },
      retry: () => {
        if (!record.pending && record.action === "retry") void save(record.attempt, true);
      },
    }));
  return {
    pendingCells: new Map(
      records
        .filter((record) => record.pending)
        .map((record) => [
          pendingKey(record.attempt.row.id, record.attempt.columnKey),
          record.attempt.display,
        ]),
    ),
    savedCells,
    onCellEdit,
    retainSource,
    statusTargets,
    keyboardProps,
    undo,
    undoPending,
    undoAvailable,
    undoOutcome,
    undoLabel:
      latest === undefined
        ? undefined
        : `${COLUMN_LABELS[latest.attempt.columnKey]} for “${latest.attempt.row.title}”`,
    feedback,
  };
}
