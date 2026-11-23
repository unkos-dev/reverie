import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useKeyboard } from "react-aria";
import { fetchQuery, graphql, useFragment, useRelayEnvironment } from "react-relay";
import { commitMutation } from "relay-runtime";

import { editingFailureAction } from "@/api/graphql";
import type { ReadingStatus } from "@/api/books";
import { invalidateLibraryVisits } from "@/lib/relay/library-visit";
import type { useCellEdit_batchRows$key } from "@/lib/relay/__generated__/useCellEdit_batchRows.graphql";
import type { useCellEditSaveMutation } from "@/lib/relay/__generated__/useCellEditSaveMutation.graphql";
import type { useCellEditReadingMutation } from "@/lib/relay/__generated__/useCellEditReadingMutation.graphql";
import type { useCellEditUndoMutation } from "@/lib/relay/__generated__/useCellEditUndoMutation.graphql";
import type { useCellEditReloadQuery } from "@/lib/relay/__generated__/useCellEditReloadQuery.graphql";
import type { useCellEditUndoEligibilityQuery } from "@/lib/relay/__generated__/useCellEditUndoEligibilityQuery.graphql";
import { READING_STATUS_INPUT, readingStatus, type EditableColumnKey } from "./edit-routing";

const batchRows = graphql`
  fragment useCellEdit_batchRows on Manifestation @relay(plural: true) @throwOnFieldError {
    id
    metadata {
      title
    }
    readingState {
      etag
      status
    }
  }
`;

const saveMutation = graphql`
  mutation useCellEditSaveMutation($input: UpdateMetadataInput!) {
    updateMetadata(input: $input) {
      __typename
      ... on MetadataSaved {
        operations {
          operationId
          fieldName
          expiresAt
        }
        manifestation {
          ...LibraryTable_cell
          ...BookCard_book
          metadata {
            etag
          }
        }
      }
      ... on StaleBaseline {
        message
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          message
          field
        }
      }
      ... on FieldsLocked {
        message
      }
      ... on EditTooLarge {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const readingMutation = graphql`
  mutation useCellEditReadingMutation($input: UpdateReadingStateInput!) {
    updateReadingState(input: $input) {
      __typename
      ... on ReadingStateUpdated {
        manifestation {
          ...LibraryTable_cell
          ...BookCard_book
          ...useCellEdit_batchRows
          readingState {
            etag
          }
        }
      }
      ... on StaleBaseline {
        message
      }
      ... on MetadataInvalid {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const undoMutation = graphql`
  mutation useCellEditUndoMutation($input: UndoMetadataInput!) {
    undoMetadata(input: $input) {
      __typename
      ... on MetadataRestored {
        manifestation {
          ...LibraryTable_cell
          ...BookCard_book
        }
      }
      ... on StaleBaseline {
        message
      }
      ... on FieldsLocked {
        message
      }
      ... on UndoConflict {
        message
      }
      ... on UndoUnavailable {
        message
      }
      ... on EditTooLarge {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const reloadQuery = graphql`
  query useCellEditReloadQuery($id: ID!) @throwOnFieldError {
    manifestation(id: $id) {
      ...LibraryTable_cell
      ...BookCard_book
      work {
        id
      }
      metadata {
        etag
        title
        subtitle
        isbn13
        pages
      }
      readingState {
        etag
        status
        rating
      }
    }
  }
`;

const eligibilityQuery = graphql`
  query useCellEditUndoEligibilityQuery($id: ID!, $operationId: UUID!) @throwOnFieldError {
    metadataOperation(id: $id, operationId: $operationId) {
      operations {
        operationId
        status
      }
      metadata {
        etag
      }
    }
  }
`;

const COLUMN_LABELS: Readonly<Record<EditableColumnKey, string>> = {
  title: "Title",
  subtitle: "Subtitle",
  isbn_13: "ISBN",
  pages: "Pages",
  authors: "Authors",
  status: "Status",
  rating: "Rating",
};

type RecoveryAction = "retry" | "reload" | "correct" | "none";
export type EditValue = string | number | readonly string[] | null;
type EditTarget = { id: string; workId?: string; title: string; baseline: string };
export type CellEditReport = {
  target: EditTarget;
  columnKey: EditableColumnKey;
  previous: EditValue;
  value: EditValue;
};
type Attempt = CellEditReport & { id: string; undoContext: object };
type EditRecord = {
  attempt: Attempt;
  pending: boolean;
  retrying: boolean;
  reloading: boolean;
  reloaded?: boolean;
  message?: string;
  action: RecoveryAction;
};
type UndoEntry = {
  attempt: Attempt;
  operationId: string;
  expiresAt: string;
  baseline: string;
};
type Owner = { enabled: boolean; context: object; entries: UndoEntry[] };
type Refusal = { action: RecoveryAction; message: string };
type SaveResult = { saved: boolean; action: RecoveryAction; message?: string };
export type StatusTarget = {
  id: string;
  attemptId: string;
  send: () => Promise<SaveResult | undefined>;
};

export function pendingKey(rowId: string, columnKey: string): string {
  return `${rowId}:${columnKey}`;
}

function displayValue(value: EditValue): string {
  if (value === null) return "unset";
  return typeof value === "object" ? value.join(", ") : String(value);
}

function undoField(field: EditableColumnKey): string {
  if (field === "authors") return "contributors.author";
  if (field === "isbn_13") return "isbn_13";
  return field;
}

function changedField(field: EditableColumnKey): string {
  return field === "authors" ? "author" : field;
}

function siblingEditions(target: EditTarget, column: EditableColumnKey) {
  if (
    target.workId === undefined ||
    (column !== "title" && column !== "subtitle" && column !== "authors")
  )
    return undefined;
  return { id: target.id, workId: target.workId };
}

function refusal(result: { __typename: string; message?: string }): Refusal {
  let action: RecoveryAction = "none";
  if (result.__typename === "StaleBaseline") action = "reload";
  else if (result.__typename === "MetadataInvalid") action = "correct";
  return { action, message: result.message ?? "Request failed." };
}

function metadataPatch(
  column: EditableColumnKey,
  value: EditValue,
): useCellEditSaveMutation["variables"]["input"]["patch"] {
  switch (column) {
    case "title":
      return { title: typeof value === "string" ? value : null };
    case "subtitle":
      return { subtitle: typeof value === "string" ? value : null };
    case "isbn_13":
      return { isbn13: typeof value === "string" ? value : null };
    case "pages":
      return { pages: typeof value === "number" ? value : null };
    case "authors":
      return {
        contributors: { author: value === null || typeof value === "object" ? value : null },
      };
    case "rating":
    case "status":
      throw new Error("Reading fields require a reading revision.");
  }
}

function runMutation<T>(
  signal: AbortSignal,
  start: (resolve: (data: T) => void, reject: (error: Error) => void) => { dispose: () => void },
): Promise<T | undefined> {
  if (signal.aborted) return Promise.resolve(undefined);
  return new Promise((resolve, reject) => {
    const dispose = start(
      (data) => {
        signal.removeEventListener("abort", abort);
        resolve(data);
      },
      (error) => {
        signal.removeEventListener("abort", abort);
        reject(error);
      },
    );
    function abort(): void {
      dispose.dispose();
      resolve(undefined);
    }
    signal.addEventListener("abort", abort, { once: true });
  });
}

type CellEditFeedback = {
  id: string;
  rowId: string;
  columnKey: EditableColumnKey;
  bookTitle: string;
  field: string;
  submitted: string;
  retained: EditValue | undefined;
  message: string;
  action: RecoveryAction;
  reloading: boolean;
  retrying: boolean;
  reload: () => void;
  retry: () => void;
};

export type UseCellEditResult = {
  pendingCells: ReadonlyMap<string, string>;
  savedCells: ReadonlySet<string>;
  onCellEdit: (report: CellEditReport) => void;
  statusTargets: (rows: readonly { id: string }[], status: ReadingStatus | null) => StatusTarget[];
  keyboardProps: ReturnType<typeof useKeyboard>["keyboardProps"];
  undo: () => void;
  undoPending: boolean;
  undoAvailable: boolean;
  undoLabel: string | undefined;
  undoOutcome: string | null;
  feedback: readonly CellEditFeedback[];
};

export function useCellEdit({
  enabled,
  rows,
}: {
  enabled: boolean;
  rows: useCellEdit_batchRows$key;
}): UseCellEditResult {
  const environment = useRelayEnvironment();
  const targets = useFragment(batchRows, rows);
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
  const active = useRef(new Set<string>());
  useEffect(() => {
    const lifetime = new AbortController();
    controller.current = lifetime;
    return () => {
      lifetime.abort();
    };
  }, []);
  const deadline = owner.entries.reduce(
    (next, entry) => Math.min(next, Date.parse(entry.expiresAt)),
    Infinity,
  );
  useEffect(() => {
    if (!Number.isFinite(deadline)) return;
    const timer = setTimeout(
      () => {
        setOwner((current) => ({
          ...current,
          entries: current.entries.filter((entry) => Date.parse(entry.expiresAt) > Date.now()),
        }));
      },
      Math.max(0, deadline - Date.now()),
    );
    return () => {
      clearTimeout(timer);
    };
  }, [deadline]);

  function setRecord(record: EditRecord): void {
    setRecords((current) => [
      ...current.filter(({ attempt }) => attempt.id !== record.attempt.id),
      record,
    ]);
  }

  async function write(attempt: Attempt, signal: AbortSignal): Promise<Refusal | null | undefined> {
    const { target, columnKey, value } = attempt;
    if (columnKey === "status" || columnKey === "rating") {
      const response = await runMutation<useCellEditReadingMutation["response"]>(
        signal,
        (ok, fail) =>
          commitMutation<useCellEditReadingMutation>(environment, {
            mutation: readingMutation,
            variables: {
              input: {
                id: target.id,
                baseline: target.baseline,
                patch:
                  columnKey === "status"
                    ? {
                        status:
                          typeof value === "string"
                            ? READING_STATUS_INPUT[value as ReadingStatus]
                            : null,
                      }
                    : { rating: typeof value === "number" ? value : null },
              },
            },
            onCompleted: ok,
            onError: fail,
          }),
      );
      if (response === undefined) return undefined;
      if (response.updateReadingState.__typename !== "ReadingStateUpdated")
        return refusal(response.updateReadingState);
    } else {
      const response = await runMutation<useCellEditSaveMutation["response"]>(signal, (ok, fail) =>
        commitMutation<useCellEditSaveMutation>(environment, {
          mutation: saveMutation,
          variables: {
            input: {
              id: target.id,
              baseline: target.baseline,
              patch: metadataPatch(columnKey, value),
              requestId: attempt.id,
            },
          },
          onCompleted: ok,
          onError: fail,
        }),
      );
      if (response === undefined) return undefined;
      const saved = response.updateMetadata;
      if (saved.__typename !== "MetadataSaved") {
        if (saved.__typename === "MetadataInvalid") {
          return {
            action: "correct",
            message:
              saved.fieldErrors.find((field) => field.field === undoField(columnKey))?.message ??
              saved.message,
          };
        }
        return refusal(saved);
      }
      const operation = saved.operations.find((entry) => entry.fieldName === undoField(columnKey));
      if (operation !== undefined) {
        const key =
          columnKey === "title" || columnKey === "subtitle" || columnKey === "authors"
            ? (target.workId ?? target.id)
            : target.id;
        setOwner((current) => {
          if (current.context !== attempt.undoContext) return current;
          const entries = current.entries.filter((entry) => {
            if (entry.attempt.columnKey !== columnKey) return true;
            const other = entry.attempt.target;
            return (
              (columnKey === "title" || columnKey === "subtitle" || columnKey === "authors"
                ? (other.workId ?? other.id)
                : other.id) !== key
            );
          });
          const baseline = saved.manifestation.metadata.etag;
          return {
            ...current,
            entries: [...entries, { attempt, ...operation, baseline }].slice(-10),
          };
        });
      }
    }
    invalidateLibraryVisits(environment, [changedField(columnKey)], {
      siblingEditions: siblingEditions(target, columnKey),
    });
    return null;
  }

  async function save(attempt: Attempt, retrying = false): Promise<SaveResult | undefined> {
    const signal = controller.current?.signal;
    const key = pendingKey(attempt.target.id, attempt.columnKey);
    if (signal === undefined || signal.aborted || active.current.has(key)) return undefined;
    active.current.add(key);
    setRecord({
      attempt,
      pending: true,
      retrying,
      reloading: false,
      action: retrying ? "retry" : "none",
    });
    setSavedCells((current) => new Set([...current].filter((saved) => saved !== key)));
    let failure: Refusal | null | undefined;
    try {
      failure = await write(attempt, signal);
    } catch (error) {
      failure = {
        action: editingFailureAction(error),
        message: error instanceof Error ? error.message : "Request failed.",
      };
    }
    active.current.delete(key);
    if (controller.current?.signal.aborted || failure === undefined) return undefined;
    if (failure === null) {
      setRecords((current) => current.filter((record) => record.attempt.id !== attempt.id));
      setSavedCells((current) => new Set(current).add(key));
      return { saved: true, action: "none" };
    }
    setRecord({ attempt, pending: false, retrying: false, reloading: false, ...failure });
    return { saved: false, ...failure };
  }

  async function reload(record: EditRecord): Promise<void> {
    const signal = controller.current?.signal;
    if (signal === undefined || signal.aborted || record.reloading) return;
    setRecord({ ...record, reloading: true });
    try {
      const data = await fetchQuery<useCellEditReloadQuery>(
        environment,
        reloadQuery,
        { id: record.attempt.target.id },
        { fetchPolicy: "network-only" },
      ).toPromise();
      if (controller.current?.signal.aborted) return;
      if (data?.manifestation == null)
        setRecord({ ...record, reloading: false, message: "This book is unavailable." });
      else {
        const id = record.attempt.target.id;
        const workId = data.manifestation.work?.id;
        invalidateLibraryVisits(environment, [], {
          reloadId: id,
          siblingEditions: workId === undefined ? undefined : { id, workId },
        });
        setRecord({ ...record, reloading: false, reloaded: true });
      }
    } catch (error) {
      if (!controller.current?.signal.aborted)
        setRecord({
          ...record,
          reloading: false,
          message: error instanceof Error ? error.message : "Reload failed.",
        });
    }
  }

  function onCellEdit(report: CellEditReport): void {
    if (!enabled || JSON.stringify(report.previous) === JSON.stringify(report.value)) return;
    const previous = records.find(
      (record) =>
        record.attempt.target.id === report.target.id &&
        record.attempt.columnKey === report.columnKey,
    );
    if (
      previous !== undefined &&
      previous.reloaded !== true &&
      JSON.stringify(previous.attempt.value) === JSON.stringify(report.value)
    )
      return;
    const target =
      previous?.action === "correct"
        ? { ...report.target, baseline: previous.attempt.target.baseline }
        : report.target;
    if (previous !== undefined)
      setRecords((current) => current.filter(({ attempt }) => attempt.id !== previous.attempt.id));
    void save({ ...report, target, id: crypto.randomUUID(), undoContext: owner.context });
  }

  function statusTargets(
    rows: readonly { id: string }[],
    status: ReadingStatus | null,
  ): StatusTarget[] {
    const ids = new Set(rows.map(({ id }) => id));
    return targets
      .filter((row) => ids.has(row.id))
      .map((row) => {
        const attempt: Attempt = {
          id: crypto.randomUUID(),
          undoContext: owner.context,
          target: { id: row.id, title: row.metadata.title, baseline: row.readingState.etag },
          columnKey: "status",
          value: status,
          previous: readingStatus(row.readingState.status),
        };
        let sent = false;
        return {
          id: row.id,
          attemptId: attempt.id,
          send: () => {
            const retrying = sent;
            sent = true;
            if (!retrying)
              setRecords((current) =>
                current.filter(
                  (record) =>
                    record.attempt.target.id !== row.id || record.attempt.columnKey !== "status",
                ),
              );
            return save(attempt, retrying);
          },
        };
      });
  }

  const latest = owner.entries.at(-1);
  const undoAvailable =
    enabled && latest !== undefined && !undoPending && !records.some((record) => record.pending);
  const latestContext = useRef(owner.context);
  useLayoutEffect(() => {
    latestContext.current = owner.context;
  }, [owner.context]);

  async function undo(): Promise<void> {
    const signal = controller.current?.signal;
    if (signal === undefined || !undoAvailable || Date.parse(latest.expiresAt) <= Date.now())
      return;
    const entry = latest;
    const { target, columnKey, undoContext } = entry.attempt;
    const send = (baseline: string) =>
      runMutation<useCellEditUndoMutation["response"]>(signal, (ok, fail) =>
        commitMutation<useCellEditUndoMutation>(environment, {
          mutation: undoMutation,
          variables: { input: { id: target.id, operationId: entry.operationId, baseline } },
          onCompleted: ok,
          onError: fail,
        }),
      );
    setUndoPending(true);
    let message: string | undefined;
    let retire = false;
    try {
      let result = (await send(entry.baseline))?.undoMetadata;
      if (result?.__typename === "StaleBaseline") {
        const refresh = await fetchQuery<useCellEditUndoEligibilityQuery>(
          environment,
          eligibilityQuery,
          { id: target.id, operationId: entry.operationId },
          { fetchPolicy: "network-only" },
        ).toPromise();
        const status = refresh?.metadataOperation.operations.find(
          (operation) => operation.operationId === entry.operationId,
        )?.status;
        if (signal.aborted || latestContext.current !== undoContext) result = undefined;
        else if (refresh !== undefined && status === "ELIGIBLE")
          result = (await send(refresh.metadataOperation.metadata.etag))?.undoMetadata;
        else {
          retire = status !== "LOCKED";
          message =
            status === "LOCKED" ? "Metadata fields are locked." : "Metadata Undo is unavailable.";
          result = undefined;
        }
      }
      if (result?.__typename === "MetadataRestored") {
        invalidateLibraryVisits(environment, [changedField(columnKey)], {
          siblingEditions: siblingEditions(target, columnKey),
        });
        retire = true;
        message = `${COLUMN_LABELS[columnKey]} for “${target.title}” restored.`;
      } else if (result !== undefined) {
        retire = result.__typename === "UndoUnavailable" || result.__typename === "UndoConflict";
        message = `Undo ${COLUMN_LABELS[columnKey]} for “${target.title}”: ${refusal(result).message}`;
      }
    } catch (error) {
      message = `Undo ${COLUMN_LABELS[columnKey]} for “${target.title}”: ${error instanceof Error ? error.message : "Request failed."}`;
    }
    if (signal.aborted) return;
    setUndoPending(false);
    if (latestContext.current !== undoContext || message === undefined) return;
    if (retire)
      setOwner((current) => ({
        ...current,
        entries: current.entries.filter((item) => item !== entry),
      }));
    setUndoOutcome(message);
  }

  const { keyboardProps } = useKeyboard({
    onKeyDown: (event) => {
      const target = event.target;
      if (
        event.key.toLowerCase() !== "z" ||
        event.shiftKey ||
        event.altKey ||
        !(event.ctrlKey || event.metaKey) ||
        (target instanceof HTMLElement &&
          (target.matches("input, textarea, select") ||
            target.isContentEditable ||
            target.closest('[role="dialog"]') !== null))
      ) {
        event.continuePropagation();
        return;
      }
      event.preventDefault();
      void undo();
    },
  });
  return {
    pendingCells: new Map(
      records
        .filter((record) => record.pending)
        .map(({ attempt }) => [
          pendingKey(attempt.target.id, attempt.columnKey),
          displayValue(attempt.value),
        ]),
    ),
    savedCells,
    onCellEdit,
    statusTargets,
    keyboardProps,
    undo: () => {
      void undo();
    },
    undoPending,
    undoAvailable,
    undoLabel:
      latest === undefined
        ? undefined
        : `${COLUMN_LABELS[latest.attempt.columnKey]} for “${latest.attempt.target.title}”`,
    undoOutcome,
    feedback: records
      .filter((record) => record.message !== undefined || record.retrying)
      .map((record) => ({
        id: record.attempt.id,
        rowId: record.attempt.target.id,
        columnKey: record.attempt.columnKey,
        bookTitle: record.attempt.target.title,
        field: COLUMN_LABELS[record.attempt.columnKey],
        submitted: displayValue(record.attempt.value),
        retained: record.action === "correct" ? record.attempt.value : undefined,
        message:
          record.action === "reload" && !record.reloading
            ? "This book changed elsewhere. Reload the latest values before editing again."
            : (record.message ?? "Request failed."),
        action: record.action,
        reloading: record.reloading,
        retrying: record.retrying,
        reload: () => {
          if (!records.some((item) => item.pending)) void reload(record);
        },
        retry: () => {
          if (!record.pending && record.action === "retry") void save(record.attempt, true);
        },
      })),
  };
}
