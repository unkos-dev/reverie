/**
 * Versions tab body for the book detail page.
 *
 * Renders pending `metadata_versions` rows grouped by `field_name`, with
 * accept / reject / revert affordances per row and an "Edit field"
 * action that opens [`EditMetadataDialog`]. Mutations invalidate the
 * `["books", "detail", id]` cache slot so the canonical fields, the
 * pending list, and the version-count badge refetch in lockstep.
 */
import {
  useMutation,
  useMutationState,
  useQueryClient,
  type MutationState,
} from "@tanstack/react-query";
import { useState, type ReactElement } from "react";
import { FieldError } from "@/components/ui/field";

import {
  ApiError,
  acceptVersion,
  rejectVersion,
  revertField,
  type BookDetail,
  type MetadataVersionRow,
} from "@/api";
import {
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { queryKeys } from "@/lib/query/keys";

import { EditMetadataDialog } from "./EditMetadataDialog";

/** Operator-visible labels for the editable canonical fields. */
const FIELD_LABEL: Record<string, string> = {
  title: "Title",
  description: "Description",
  language: "Language",
  publisher: "Publisher",
  pub_date: "Publication date",
  isbn_10: "ISBN-10",
  isbn_13: "ISBN-13",
};

type VersionsTabProps = {
  book: BookDetail;
};

type FieldActionVariables = { field: string } & (
  | { action: "clear" }
  | { row: MetadataVersionRow; action: "accept" | "reject" }
);
type FieldActionState = MutationState<void, Error, FieldActionVariables>;

function fieldActionKey(manifestationId: string) {
  return [...queryKeys.books.detail(manifestationId), "version-action"] as const;
}

/**
 * Versions tab with field-owned action outcomes and an Edit dialog.
 */
export function VersionsTab({ book }: Readonly<VersionsTabProps>): ReactElement {
  const [editOpen, setEditOpen] = useState(false);
  const grouped = groupByField(book.metadata_versions);
  const actions = useMutationState<FieldActionState>({
    filters: { mutationKey: fieldActionKey(book.id) },
  });
  const retainedFields = actions.flatMap((action) =>
    (action.status === "pending" || action.status === "error") && action.variables !== undefined
      ? [action.variables.field]
      : [],
  );
  // Name/label defs only: EditMetadataDialog seeds each field's canonical
  // value itself from a fresh GET, not from this (possibly stale) `book`.
  const editableFields = (
    ["title", "description", "language", "publisher", "pub_date", "isbn_10", "isbn_13"] as const
  ).map((name) => ({
    name,
    label: FIELD_LABEL[name] ?? name,
  }));
  const fieldNames = [
    ...new Set([
      ...editableFields.map(({ name }) => name),
      ...Object.keys(grouped),
      ...retainedFields,
    ]),
  ].sort((a, b) => a.localeCompare(b));

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <p className="text-fg-muted text-sm">
          {book.metadata_version_summary.pending === 0
            ? "No pending drafts. Manually edit any field below."
            : `${String(book.metadata_version_summary.pending)} pending draft${book.metadata_version_summary.pending === 1 ? "" : "s"} await review.`}
        </p>
        <Button
          variant="outline"
          onPress={() => {
            setEditOpen(true);
          }}
        >
          Edit metadata
        </Button>
      </div>

      <div className="space-y-6">
        {fieldNames.map((field) => (
          <FieldGroup
            key={field}
            manifestationId={book.id}
            field={field}
            canonical={canonicalValue(book, field)}
            rows={grouped[field] ?? []}
            actionState={actions.findLast((action) => action.variables?.field === field)}
          />
        ))}
      </div>

      <EditMetadataDialog
        manifestationId={book.id}
        open={editOpen}
        onOpenChange={setEditOpen}
        fields={editableFields}
      />
    </div>
  );
}

type FieldGroupProps = {
  manifestationId: string;
  field: string;
  canonical: string | null;
  rows: MetadataVersionRow[];
  actionState: FieldActionState | undefined;
};

/** One field's section: current canonical row + pending alternatives. */
function FieldGroup({
  manifestationId,
  field,
  canonical,
  rows,
  actionState,
}: Readonly<FieldGroupProps>): ReactElement | null {
  const queryClient = useQueryClient();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const pending = actionState?.status === "pending";
  const submitted = actionState?.variables;
  const clearPending = pending && submitted?.action === "clear";
  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: queryKeys.books.all,
    });

  const mutationKey = [...fieldActionKey(manifestationId), field];
  const action = useMutation({
    mutationKey,
    mutationFn: (variables: FieldActionVariables) => {
      if (variables.action === "clear") return revertField(manifestationId, variables.field, null);
      return variables.action === "accept"
        ? acceptVersion(manifestationId, variables.row.id)
        : rejectVersion(manifestationId, variables.row.id);
    },
    onSuccess: () => {
      const cache = queryClient.getMutationCache();
      for (const failure of cache.findAll({ mutationKey, exact: true, status: "error" })) {
        cache.remove(failure);
      }
      return invalidate();
    },
    onError: (error) => {
      console.error("[VersionsTab] field action failed", error);
      void invalidate();
    },
  });
  if (rows.length === 0 && actionState?.status !== "error" && !pending) return null;
  return (
    <section
      className="border-border space-y-3 rounded-md border p-4"
      aria-labelledby={`versions-${field}-heading`}
    >
      <header className="flex items-center justify-between gap-3">
        <h3
          id={`versions-${field}-heading`}
          className="text-fg text-sm font-medium uppercase tracking-wide"
        >
          {FIELD_LABEL[field] ?? field}
        </h3>
        {(canonical !== null || clearPending) && field !== "title" ? (
          <Button
            size="sm"
            variant="ghost"
            onPress={() => {
              setConfirmOpen(true);
            }}
            isPending={clearPending}
            isDisabled={pending && !clearPending}
          >
            Clear
          </Button>
        ) : null}
      </header>
      <ul className="space-y-2 text-sm">
        <CanonicalRow canonical={canonical} />
        {rows.map((row) => (
          <PendingRow
            key={row.id}
            row={row}
            accepting={pending && submitted?.action === "accept" && submitted.row.id === row.id}
            rejecting={pending && submitted?.action === "reject" && submitted.row.id === row.id}
            busy={pending}
            onAccept={() => {
              action.mutate({ field, row, action: "accept" });
            }}
            onReject={() => {
              action.mutate({ field, row, action: "reject" });
            }}
          />
        ))}
      </ul>

      <FieldOutcome actionState={actionState} rows={rows} />
      <AlertDialogContent isOpen={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogHeader>
          <AlertDialogTitle>Clear {FIELD_LABEL[field] ?? field}?</AlertDialogTitle>
          <AlertDialogDescription>
            The canonical value will be set to NULL and an audit row recorded. Continue?
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onPress={() => {
              action.mutate({ field, action: "clear" });
              setConfirmOpen(false);
            }}
          >
            Clear
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </section>
  );
}

function FieldOutcome({
  actionState,
  rows,
}: Readonly<Pick<FieldGroupProps, "actionState" | "rows">>): ReactElement | null {
  if (actionState === undefined || actionState.variables === undefined) return null;
  const variables = actionState.variables;
  if (variables.action === "clear") {
    return actionState.status === "error" ? (
      <FieldError>Clear failed: {formatError(actionState.error)}</FieldError>
    ) : null;
  }
  const { row, action } = variables;
  if (actionState.status === "error")
    return (
      <FieldError>
        {row.source} draft:{" "}
        {actionState.error instanceof ApiError && actionState.error.status === 404
          ? "This draft is no longer available."
          : formatError(actionState.error)}
      </FieldError>
    );
  if (actionState.status === "pending" && !rows.some((pending) => pending.id === row.id))
    return (
      <p role="status" className="text-fg-muted text-sm">
        {action === "accept" ? "Accepting" : "Rejecting"} {row.source} draft…
      </p>
    );
  return null;
}

type CanonicalRowProps = {
  canonical: string | null;
};

function CanonicalRow({ canonical }: Readonly<CanonicalRowProps>): ReactElement {
  return (
    <li className="border-border bg-surface-1 flex items-start justify-between gap-4 rounded-sm border p-3">
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs uppercase tracking-wide">Current</p>
        <p className="text-fg mt-1 break-words">
          {canonical ?? <span className="italic">unset</span>}
        </p>
      </div>
      <Badge variant="default">canonical</Badge>
    </li>
  );
}

type PendingRowProps = {
  row: MetadataVersionRow;
  accepting: boolean;
  rejecting: boolean;
  busy: boolean;
  onAccept: () => void;
  onReject: () => void;
};

function PendingRow({
  row,
  accepting,
  rejecting,
  busy,
  onAccept,
  onReject,
}: Readonly<PendingRowProps>): ReactElement {
  const value = renderValue(row.new_value);

  return (
    <li className="border-border flex items-start justify-between gap-4 rounded-sm border p-3">
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs uppercase tracking-wide">
          {row.source} · {row.match_type} · {Math.round(row.confidence_score * 100).toString()}%
        </p>
        <p className="text-fg mt-1 break-words">{value}</p>
      </div>
      <div className="flex shrink-0 gap-2">
        <Button size="sm" onPress={onAccept} isPending={accepting} isDisabled={busy && !accepting}>
          Accept
        </Button>
        <Button
          size="sm"
          variant="outline"
          onPress={onReject}
          isPending={rejecting}
          isDisabled={busy && !rejecting}
        >
          Reject
        </Button>
      </div>
    </li>
  );
}

function groupByField(rows: readonly MetadataVersionRow[]): Record<string, MetadataVersionRow[]> {
  const out: Record<string, MetadataVersionRow[]> = {};
  for (const row of rows) {
    const slot = out[row.field_name] ?? [];
    slot.push(row);
    out[row.field_name] = slot;
  }
  return out;
}

function canonicalValue(book: BookDetail, field: string): string | null {
  switch (field) {
    case "title":
      return book.title;
    case "description":
      return book.description;
    case "language":
      return book.language;
    case "isbn_10":
      return book.isbn_10;
    case "isbn_13":
      return book.isbn_13;
    case "publisher":
      return book.publisher;
    case "pub_date":
      return book.pub_date;
    default:
      return null;
  }
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined) return "(null)";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function formatError(err: unknown): string {
  if (err instanceof ApiError) return `${err.title}: ${err.detail}`;
  if (err instanceof Error) return err.message;
  return "Request failed.";
}
