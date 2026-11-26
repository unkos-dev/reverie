import { useState } from "react";
import { graphql, useFragment, useMutation, useRelayEnvironment } from "react-relay";
import type { ReadOnlyRecordProxy } from "relay-runtime";

import type { ReadingStatus } from "@/api/books";
import { invalidateAfterMetadataEdit, invalidateLibraryVisits } from "@/lib/relay/library-visit";
import type { useCellEdit_batchRows$key } from "@/lib/relay/__generated__/useCellEdit_batchRows.graphql";
import type { useCellEditSaveMutation } from "@/lib/relay/__generated__/useCellEditSaveMutation.graphql";
import type { useCellEditReadingMutation } from "@/lib/relay/__generated__/useCellEditReadingMutation.graphql";
import { READING_STATUS_INPUT, type EditableColumnKey } from "./edit-routing";

const batchRows = graphql`
  fragment useCellEdit_batchRows on Manifestation @relay(plural: true) @throwOnFieldError {
    id
    readingState {
      etag
    }
  }
`;

const saveMutation = graphql`
  mutation useCellEditSaveMutation($input: UpdateMetadataInput!) {
    updateMetadata(input: $input) {
      __typename
      ... on MetadataSaved {
        manifestation {
          isbn10
          isbn13
          isbnMismatch
          ...LibraryTable_cell
          ...BookCard_book
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

const COLUMN_LABELS: Readonly<Record<EditableColumnKey, string>> = {
  title: "Title",
  subtitle: "Subtitle",
  isbn_13: "ISBN",
  pages: "Pages",
  authors: "Authors",
  status: "Status",
  rating: "Rating",
};
const EMPTY_PENDING_CELLS: ReadonlyMap<string, string> = new Map();

export type EditValue = string | number | readonly string[] | null;
type EditTarget = { id: string; workId?: string; title: string; baseline: string };
export type CellEditReport = {
  target: EditTarget;
  columnKey: EditableColumnKey;
  previous: EditValue;
  value: EditValue;
};
export function pendingKey(rowId: string, columnKey: string): string {
  return `${rowId}:${columnKey}`;
}

function displayValue(value: EditValue): string {
  if (value === null) return "unset";
  return typeof value === "object" ? value.join(", ") : String(value);
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

type StatusTarget = { id: string; baseline: string };

export type UseCellEditResult = {
  pendingCells: ReadonlyMap<string, string>;
  savedCells: ReadonlySet<string>;
  onCellEdit: (report: CellEditReport) => void;
  statusTargets: (rows: readonly { id: string }[]) => StatusTarget[];
  saveStatus: (target: StatusTarget, status: ReadingStatus | null) => Promise<boolean | undefined>;
  error: string | null;
  dismissError: () => void;
};

function refusalMessage(result: { __typename: string; message?: string }): string {
  return result.message ?? "Request failed.";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Request failed.";
}

export function useCellEdit({
  enabled,
  rows,
}: {
  enabled: boolean;
  rows: useCellEdit_batchRows$key;
}): UseCellEditResult {
  const environment = useRelayEnvironment();
  const targets = useFragment(batchRows, rows);
  const [commitMetadata, metadataInFlight] = useMutation<useCellEditSaveMutation>(saveMutation);
  const [commitReading, readingInFlight] = useMutation<useCellEditReadingMutation>(readingMutation);
  const [submittedCells, setSubmittedCells] = useState<ReadonlyMap<string, string>>(new Map());
  const [savedCells, setSavedCells] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  function onCellEdit({ target, columnKey, previous, value }: CellEditReport): void {
    const key = pendingKey(target.id, columnKey);
    if (!enabled || submittedCells.has(key) || JSON.stringify(previous) === JSON.stringify(value))
      return;

    setError(null);
    setSubmittedCells((current) => new Map(current).set(key, displayValue(value)));
    setSavedCells((current) => new Set([...current].filter((saved) => saved !== key)));

    function releaseCell(): void {
      setSubmittedCells((current) => {
        const next = new Map(current);
        next.delete(key);
        return next;
      });
    }

    function complete(failure?: { message: string; stale?: boolean }): void {
      releaseCell();
      if (failure === undefined) {
        const field = columnKey === "authors" ? "contributors.author" : columnKey;
        if (columnKey === "status" || columnKey === "rating")
          invalidateLibraryVisits(environment, [columnKey]);
        else invalidateAfterMetadataEdit(environment, [field], target.id, target.workId);
        setSavedCells((current) => new Set(current).add(key));
      } else {
        if (failure.stale === true) {
          invalidateLibraryVisits(environment, [], {
            reloadId: target.id,
            siblingEditions:
              columnKey === "status" || columnKey === "rating" || target.workId === undefined
                ? undefined
                : { id: target.id, workId: target.workId },
          });
        }
        setError(`${COLUMN_LABELS[columnKey]} for “${target.title}”: ${failure.message}`);
      }
    }
    const input = { id: target.id, baseline: target.baseline };
    const onError = (error: Error) => {
      complete({ message: errorMessage(error) });
    };

    if (columnKey === "status" || columnKey === "rating") {
      commitReading({
        variables: {
          input: {
            ...input,
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
        onCompleted: (data: useCellEditReadingMutation["response"] | undefined) => {
          if (data === undefined) {
            releaseCell();
            return;
          }
          const result = data.updateReadingState;
          complete(
            result.__typename === "ReadingStateUpdated"
              ? undefined
              : { message: refusalMessage(result), stale: result.__typename === "StaleBaseline" },
          );
        },
        onError,
      });
      return;
    }

    commitMetadata({
      variables: { input: { ...input, patch: metadataPatch(columnKey, value) } },
      updater: (store, data) => {
        if (data?.updateMetadata.__typename === "MetadataSaved") {
          const edition: ReadOnlyRecordProxy | null | undefined = store.get(target.id);
          edition?.getLinkedRecord("versions")?.invalidateRecord();
        }
      },
      onCompleted: (data: useCellEditSaveMutation["response"] | undefined) => {
        if (data === undefined) {
          releaseCell();
          return;
        }
        const result = data.updateMetadata;
        if (result.__typename === "MetadataSaved") {
          complete();
          return;
        }
        const fieldName = columnKey === "authors" ? "contributors.author" : columnKey;
        const fieldMessage =
          result.__typename === "MetadataInvalid"
            ? result.fieldErrors.find((field) => field.field === fieldName)?.message
            : undefined;
        complete({
          message: fieldMessage ?? refusalMessage(result),
          stale: result.__typename === "StaleBaseline",
        });
      },
      onError,
    });
  }

  function statusTargets(rows: readonly { id: string }[]): StatusTarget[] {
    const ids = new Set(rows.map(({ id }) => id));
    return targets
      .filter((row) => ids.has(row.id))
      .map((row) => ({ id: row.id, baseline: row.readingState.etag }));
  }

  function saveStatus(
    { id, baseline }: StatusTarget,
    status: ReadingStatus | null,
  ): Promise<boolean | undefined> {
    return new Promise((resolve) => {
      commitReading({
        variables: {
          input: {
            id,
            baseline,
            patch: { status: status === null ? null : READING_STATUS_INPUT[status] },
          },
        },
        onCompleted: (data: useCellEditReadingMutation["response"] | undefined) => {
          if (data === undefined) {
            resolve(undefined);
            return;
          }
          const result = data.updateReadingState;
          if (result.__typename === "ReadingStateUpdated")
            invalidateLibraryVisits(environment, ["status"]);
          if (result.__typename === "StaleBaseline")
            invalidateLibraryVisits(environment, [], { reloadId: id });
          resolve(result.__typename === "ReadingStateUpdated");
        },
        onError: () => {
          resolve(false);
        },
      });
    });
  }

  return {
    pendingCells: metadataInFlight || readingInFlight ? submittedCells : EMPTY_PENDING_CELLS,
    savedCells,
    onCellEdit,
    statusTargets,
    saveStatus,
    error,
    dismissError: () => {
      setError(null);
    },
  };
}
