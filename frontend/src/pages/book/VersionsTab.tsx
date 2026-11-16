/** Pending metadata proposals and field-owned review outcomes. */
import { useEffect, useRef, useState, useTransition, type ReactElement } from "react";
import {
  useLazyLoadQuery,
  usePaginationFragment,
  useRelayEnvironment,
  useSubscribeToInvalidationState,
} from "react-relay";
import { commitLocalUpdate } from "relay-runtime";
import { encodeGraphQLId, executeEditingOnce, GraphQLExecutionError } from "@/api/graphql";
import { readerVersions, readerVersionsQuery } from "@/lib/relay/reader";
import {
  mutationsAcceptMetadataVersionMutation,
  mutationsRejectMetadataVersionMutation,
  mutationsRevertMetadataFieldMutation,
} from "@/lib/relay/mutations";
import type { readerVersionsQuery as VersionsQuery } from "@/lib/relay/__generated__/readerVersionsQuery.graphql";
import type { readerVersionsPaginationQuery as VersionsPagination } from "@/lib/relay/__generated__/readerVersionsPaginationQuery.graphql";
import type { readerVersions$key } from "@/lib/relay/__generated__/readerVersions.graphql";
import type { mutationsAcceptMetadataVersionMutation as AcceptVersion } from "@/lib/relay/__generated__/mutationsAcceptMetadataVersionMutation.graphql";
import type { mutationsRejectMetadataVersionMutation as RejectVersion } from "@/lib/relay/__generated__/mutationsRejectMetadataVersionMutation.graphql";
import type { mutationsRevertMetadataFieldMutation as RevertField } from "@/lib/relay/__generated__/mutationsRevertMetadataFieldMutation.graphql";
import { FieldError } from "@/components/ui/field";

import { ApiError, type BookDetail } from "@/api";
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
  book: Pick<
    BookDetail,
    "id" | "title" | "description" | "language" | "isbn_10" | "isbn_13" | "publisher" | "pub_date"
  >;
};

type FieldActionVariables = { field: string } & (
  | { action: "clear" }
  | { row: MetadataVersionRow; action: "accept" | "reject" }
);
type MetadataVersionRow = {
  id: string;
  field_name: string;
  source: string;
  match_type: string;
  confidence_score: number;
  new_value: unknown;
};
type FieldActionState = {
  status: "pending" | "success" | "error";
  variables: FieldActionVariables;
  error?: Error;
};

/**
 * Versions tab with field-owned action outcomes and an Edit dialog.
 */
export function VersionsTab({ book }: Readonly<VersionsTabProps>): ReactElement {
  const [editOpen, setEditOpen] = useState(false);
  const query = useLazyLoadQuery<VersionsQuery>(readerVersionsQuery, {
    id: encodeGraphQLId("Manifestation", book.id),
  });
  const { data, hasNext, isLoadingNext, loadNext, refetch } = usePaginationFragment<
    VersionsPagination,
    readerVersions$key
  >(readerVersions, query.manifestation);
  const [pageError, setPageError] = useState<Error | null>(null);
  const [, startTransition] = useTransition();
  useEffect(() => {
    if (hasNext && !isLoadingNext)
      loadNext(20, {
        onComplete: (error) => {
          if (error !== null) setPageError(error);
        },
      });
  }, [hasNext, isLoadingNext, loadNext]);
  if (pageError !== null) throw pageError;
  const rows =
    data?.versions.pending.edges.map(({ node }) => ({
      id: node.id,
      field_name: node.fieldName,
      source: node.source,
      match_type: node.matchType,
      confidence_score: node.confidenceScore,
      new_value: node.newValue,
    })) ?? [];
  const grouped = groupByField(rows);
  const [actions, setActions] = useState<FieldActionState[]>([]);
  const refresh = (): void => {
    startTransition(() => {
      refetch({}, { fetchPolicy: "network-only" });
    });
  };
  useSubscribeToInvalidationState([encodeGraphQLId("Manifestation", book.id)], refresh);
  const retainedFields = actions.flatMap((action) =>
    action.status === "pending" || action.status === "error" ? [action.variables.field] : [],
  );
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
          {(data?.versions.pendingCount ?? 0) === 0
            ? "No pending drafts. Manually edit any field below."
            : `${String(data?.versions.pendingCount ?? 0)} pending draft${(data?.versions.pendingCount ?? 0) === 1 ? "" : "s"} await review.`}
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
            actionState={actions.findLast((action) => action.variables.field === field)}
            onAction={(action) => {
              setActions((current) => [
                ...current.filter(
                  (previous) => previous.variables.field !== action.variables.field,
                ),
                action,
              ]);
            }}
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
  onAction: (action: FieldActionState) => void;
};

/** One field's section: current canonical row + pending alternatives. */
function FieldGroup({
  manifestationId,
  field,
  canonical,
  rows,
  actionState,
  onAction,
}: Readonly<FieldGroupProps>): ReactElement | null {
  const environment = useRelayEnvironment();
  const controller = useRef<AbortController | null>(null);
  useEffect(() => {
    const lifetime = new AbortController();
    controller.current = lifetime;
    return () => {
      lifetime.abort();
    };
  }, []);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const pending = actionState?.status === "pending";
  const submitted = actionState?.variables;
  const clearPending = pending && submitted?.action === "clear";
  async function mutate(variables: FieldActionVariables): Promise<void> {
    if (pending) return;
    onAction({ status: "pending", variables });
    const id = encodeGraphQLId("Manifestation", manifestationId);
    try {
      let metadata: AcceptVersion["response"]["acceptMetadataVersion"]["metadata"];
      if (variables.action === "clear") {
        const response = await executeEditingOnce<RevertField>(
          environment,
          mutationsRevertMetadataFieldMutation,
          { id, fieldName: variables.field, versionId: null },
          controller.current?.signal,
        );
        if (response === undefined) return;
        metadata = response.revertMetadataField.metadata;
      } else if (variables.action === "accept") {
        const response = await executeEditingOnce<AcceptVersion>(
          environment,
          mutationsAcceptMetadataVersionMutation,
          { id, versionId: variables.row.id },
          controller.current?.signal,
        );
        if (response === undefined) return;
        metadata = response.acceptMetadataVersion.metadata;
      } else {
        const response = await executeEditingOnce<RejectVersion>(
          environment,
          mutationsRejectMetadataVersionMutation,
          { id, versionId: variables.row.id },
          controller.current?.signal,
        );
        if (response === undefined) return;
      }
      commitLocalUpdate(environment, (store) => {
        const edition = store.get(id);
        const work =
          metadata == null ? edition?.getLinkedRecord("work") : store.get(metadata.workId);
        if (metadata != null) {
          for (const key of [
            "title",
            "subtitle",
            "description",
            "language",
            "contentRating",
          ] as const)
            work?.setValue(metadata[key], key);
          for (const key of [
            "isbn10",
            "isbn13",
            "isbnMismatch",
            "publisher",
            "publicationDate",
            "pages",
          ] as const)
            edition?.setValue(metadata[key], key);
        }
        work?.invalidateRecord();
        edition?.invalidateRecord();
      });
      onAction({ status: "success", variables });
    } catch (failure) {
      if (controller.current?.signal.aborted) return;
      commitLocalUpdate(environment, (store) => {
        const edition = store.get(id);
        const work = edition?.getLinkedRecord("work");
        work?.invalidateRecord();
        edition?.invalidateRecord();
      });
      console.error("[VersionsTab] field action failed", failure);
      onAction({
        status: "error",
        variables,
        error: failure instanceof Error ? failure : new Error("Request failed."),
      });
    }
  }
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
              void mutate({ field, row, action: "accept" });
            }}
            onReject={() => {
              void mutate({ field, row, action: "reject" });
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
              void mutate({ field, action: "clear" });
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
  if (actionState === undefined) return null;
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
        {(actionState.error instanceof ApiError && actionState.error.status === 404) ||
        (actionState.error instanceof GraphQLExecutionError &&
          actionState.error.errors.some((error) => error.extensions?.code === "not-found"))
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
    <li className="border-border bg-surface flex items-start justify-between gap-4 rounded-sm border p-3">
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

function canonicalValue(book: VersionsTabProps["book"], field: string): string | null {
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
