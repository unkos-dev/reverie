/** Pending metadata proposals and field-owned review outcomes. */
import { useEffect, useState, useTransition, type ReactElement } from "react";
import {
  graphql,
  useFragment,
  useMutation,
  usePaginationFragment,
  usePreloadedQuery,
  useQueryLoader,
  useRelayEnvironment,
  useSubscribeToInvalidationState,
  type PreloadedQuery,
} from "react-relay";
import { commitLocalUpdate } from "relay-runtime";
import { invalidateAfterMetadataEdit } from "@/lib/relay/library-visit";
import editMetadataDialogQuery, {
  type EditMetadataDialogQuery,
} from "@/lib/relay/__generated__/EditMetadataDialogQuery.graphql";
import type { VersionsTabQuery } from "@/lib/relay/__generated__/VersionsTabQuery.graphql";
import type {
  VersionsTab_book$data,
  VersionsTab_book$key,
} from "@/lib/relay/__generated__/VersionsTab_book.graphql";
import type { VersionsTabPaginationQuery } from "@/lib/relay/__generated__/VersionsTabPaginationQuery.graphql";
import type {
  VersionsTab_proposals$data,
  VersionsTab_proposals$key,
} from "@/lib/relay/__generated__/VersionsTab_proposals.graphql";
import type {
  VersionsTab_proposal$data,
  VersionsTab_proposal$key,
} from "@/lib/relay/__generated__/VersionsTab_proposal.graphql";
import type { VersionsTabAcceptMutation } from "@/lib/relay/__generated__/VersionsTabAcceptMutation.graphql";
import type { VersionsTabRejectMutation } from "@/lib/relay/__generated__/VersionsTabRejectMutation.graphql";
import type { VersionsTabClearMutation } from "@/lib/relay/__generated__/VersionsTabClearMutation.graphql";
import { FieldError } from "@/components/ui/field";

import { ApiError } from "@/api";
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

const versionsQuery = graphql`
  query VersionsTabQuery($id: ID!, $count: Int = 20, $cursor: String) @throwOnFieldError {
    manifestation(id: $id) {
      work {
        id
      }
      ...VersionsTab_proposals @arguments(count: $count, cursor: $cursor)
    }
  }
`;

const versionsBook = graphql`
  fragment VersionsTab_book on Manifestation @throwOnFieldError {
    id
    isbn10
    isbn13
    publisher
    publicationDate
    work {
      title
      description
      language
    }
  }
`;

const proposalsFragment = graphql`
  fragment VersionsTab_proposals on Manifestation
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "VersionsTabPaginationQuery", preferFetchable: true) {
    versions {
      __id
      pendingCount
      pending(first: $count, after: $cursor) @connection(key: "VersionsTab_proposals_pending") {
        edges {
          node {
            id
            fieldName
            ...VersionsTab_proposal
          }
        }
      }
    }
  }
`;

const proposalFragment = graphql`
  fragment VersionsTab_proposal on MetadataVersion @throwOnFieldError {
    id
    source
    matchType
    confidenceScore
    newValue {
      __typename
      ... on MetadataProposalText {
        text
      }
      ... on MetadataProposalInteger {
        integer
      }
      ... on MetadataProposalStringList {
        strings
      }
      ... on MetadataProposalSeries {
        name
        position
      }
      ... on MetadataProposalContributors {
        contributors {
          name
          code
        }
      }
    }
  }
`;

const acceptVersionMutation = graphql`
  mutation VersionsTabAcceptMutation($input: AcceptMetadataVersionInput!) {
    acceptMetadataVersion(input: $input) {
      __typename
      ... on MetadataVersionAccepted {
        manifestation {
          id
          work {
            id
          }
          metadata {
            ...EditMetadataDialog_opening
          }
          ...LibraryTable_cell
          ...BookPage_book
          ...VersionsTab_proposals
        }
      }
      ... on FieldsLocked {
        message
        fields
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          field
          message
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const rejectVersionMutation = graphql`
  mutation VersionsTabRejectMutation($input: RejectMetadataVersionInput!) {
    rejectMetadataVersion(input: $input) {
      __typename
      ... on MetadataVersionRejected {
        manifestation {
          id
          ...BookPage_book
          ...VersionsTab_proposals
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

const clearVersionMutation = graphql`
  mutation VersionsTabClearMutation($input: RevertMetadataFieldInput!) {
    revertMetadataField(input: $input) {
      __typename
      ... on MetadataFieldReverted {
        manifestation {
          id
          work {
            id
          }
          metadata {
            ...EditMetadataDialog_opening
          }
          ...LibraryTable_cell
          ...BookPage_book
          ...VersionsTab_proposals
        }
      }
      ... on FieldsLocked {
        message
        fields
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          field
          message
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

type Proposal = NonNullable<
  VersionsTab_proposals$data["versions"]
>["pending"]["edges"][number]["node"];
type VersionIdentity = { id: string; source: string };
type FieldActionVariables = { field: string } & (
  | { action: "clear" }
  | { version: VersionIdentity; action: "accept" | "reject" }
);
type FieldActionState = {
  status: "submitted" | "success" | "error";
  variables: FieldActionVariables;
  message?: string;
};

type VersionsTabProps = {
  book: VersionsTab_book$key;
  reference: PreloadedQuery<VersionsTabQuery>;
};

export function VersionsTab({
  book: bookKey,
  reference,
}: Readonly<VersionsTabProps>): ReactElement {
  const book = useFragment(versionsBook, bookKey);
  const [editOpen, setEditOpen] = useState(false);
  const [formRef, loadForm, disposeForm] =
    useQueryLoader<EditMetadataDialogQuery>(editMetadataDialogQuery);
  const query = usePreloadedQuery<VersionsTabQuery>(versionsQuery, reference);
  const { data, hasNext, isLoadingNext, loadNext, refetch } = usePaginationFragment<
    VersionsTabPaginationQuery,
    VersionsTab_proposals$key
  >(proposalsFragment, query.manifestation);
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
  const grouped = new Map<string, Proposal[]>();
  for (const { node } of data?.versions.pending.edges ?? []) {
    const rows = grouped.get(node.fieldName);
    if (rows === undefined) grouped.set(node.fieldName, [node]);
    else rows.push(node);
  }
  const [actions, setActions] = useState<FieldActionState[]>([]);
  const refresh = (): void => {
    startTransition(() => {
      refetch({}, { fetchPolicy: "network-only" });
    });
  };
  useSubscribeToInvalidationState(
    [book.id, query.manifestation?.work?.id, data?.versions.__id].filter((id) => id !== undefined),
    refresh,
  );
  const retainedFields = actions.flatMap((action) =>
    action.status === "submitted" || action.status === "error" ? [action.variables.field] : [],
  );
  const editableFields = (
    ["title", "description", "language", "publisher", "pub_date", "isbn_10", "isbn_13"] as const
  ).map((name) => ({
    name,
    label: FIELD_LABEL[name] ?? name,
  }));
  const fieldNames = [
    ...new Set([...editableFields.map(({ name }) => name), ...grouped.keys(), ...retainedFields]),
  ].sort((a, b) => a.localeCompare(b));

  const pendingCount = data?.versions.pendingCount ?? 0;
  const plural = pendingCount === 1 ? "" : "s";
  let pendingText = "No pending drafts. Manually edit any field below.";
  if (pendingCount !== 0)
    pendingText = `${String(pendingCount)} pending draft${plural} await review.`;

  function recordAction(action: FieldActionState): void {
    const replaceAction = (current: FieldActionState[]): FieldActionState[] => [
      ...current.filter((previous) => previous.variables.field !== action.variables.field),
      action,
    ];
    setActions(replaceAction);
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <p className="text-fg-muted text-sm">{pendingText}</p>
        <Button
          variant="outline"
          onPress={() => {
            loadForm({ id: book.id });
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
            rows={grouped.get(field) ?? []}
            actionState={actions.findLast((action) => action.variables.field === field)}
            onAction={recordAction}
          />
        ))}
      </div>

      <EditMetadataDialog
        queryRef={formRef}
        load={(fetchPolicy) => {
          loadForm({ id: book.id }, { fetchPolicy });
        }}
        open={editOpen}
        onOpenChange={(open) => {
          if (!open) disposeForm();
          setEditOpen(open);
        }}
        fields={editableFields}
      />
    </div>
  );
}

type FieldGroupProps = {
  manifestationId: string;
  field: string;
  canonical: string | null;
  rows: readonly Proposal[];
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
  const [commitAccept, accepting] = useMutation<VersionsTabAcceptMutation>(acceptVersionMutation);
  const [commitReject, rejecting] = useMutation<VersionsTabRejectMutation>(rejectVersionMutation);
  const [commitRevert, clearing] = useMutation<VersionsTabClearMutation>(clearVersionMutation);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const pending = accepting || rejecting || clearing;
  const submitted = actionState?.variables;
  const clearPending = clearing;
  function failed(variables: FieldActionVariables, message: string): void {
    commitLocalUpdate(environment, (store) => {
      store.get(manifestationId)?.invalidateRecord();
    });
    onAction({ status: "error", variables, message });
  }
  function mutate(variables: FieldActionVariables): void {
    if (pending) return;
    onAction({ status: "submitted", variables });
    const completed = (
      result:
        | VersionsTabAcceptMutation["response"]["acceptMetadataVersion"]
        | VersionsTabRejectMutation["response"]["rejectMetadataVersion"]
        | VersionsTabClearMutation["response"]["revertMetadataField"],
    ): void => {
      switch (result.__typename) {
        case "MetadataVersionAccepted":
        case "MetadataFieldReverted": {
          const { id, work } = result.manifestation;
          invalidateAfterMetadataEdit(environment, [variables.field], id, work?.id);
          onAction({ status: "success", variables });
          break;
        }
        case "MetadataVersionRejected":
          onAction({ status: "success", variables });
          break;
        case "NotFound":
          failed(
            variables,
            variables.action === "clear" ? result.message : "This draft is no longer available.",
          );
          break;
        case "MetadataInvalid":
        case "FieldsLocked":
          failed(variables, result.message);
          break;
        default:
          failed(variables, "Request failed.");
      }
    };
    const onError = (error: Error): void => {
      console.error("[VersionsTab] field action failed", error);
      failed(variables, formatError(error));
    };
    const id = manifestationId;
    if (variables.action === "clear")
      commitRevert({
        variables: { input: { id, fieldName: variables.field, versionId: null } },
        onCompleted: (response) => {
          completed(response.revertMetadataField);
        },
        onError,
      });
    else if (variables.action === "accept")
      commitAccept({
        variables: { input: { id, versionId: variables.version.id } },
        onCompleted: (response) => {
          completed(response.acceptMetadataVersion);
        },
        onError,
      });
    else
      commitReject({
        variables: { input: { id, versionId: variables.version.id } },
        onCompleted: (response) => {
          completed(response.rejectMetadataVersion);
        },
        onError,
      });
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
            accepting={
              accepting && submitted?.action === "accept" && submitted.version.id === row.id
            }
            rejecting={
              rejecting && submitted?.action === "reject" && submitted.version.id === row.id
            }
            busy={pending}
            onAccept={(version) => {
              mutate({ field, version, action: "accept" });
            }}
            onReject={(version) => {
              mutate({ field, version, action: "reject" });
            }}
          />
        ))}
      </ul>

      <FieldOutcome actionState={actionState} pending={pending} rows={rows} />
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
              mutate({ field, action: "clear" });
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
  pending,
  rows,
}: Readonly<
  Pick<FieldGroupProps, "actionState" | "rows"> & { pending: boolean }
>): ReactElement | null {
  if (actionState === undefined) return null;
  const variables = actionState.variables;
  if (variables.action === "clear") {
    return actionState.status === "error" ? (
      <FieldError>Clear failed: {actionState.message}</FieldError>
    ) : null;
  }
  const { version, action } = variables;
  if (actionState.status === "error")
    return (
      <FieldError>
        {version.source} draft: {actionState.message}
      </FieldError>
    );
  if (pending && !rows.some((pendingRow) => pendingRow.id === version.id))
    return (
      <p role="status" className="text-fg-muted text-sm">
        {action === "accept" ? "Accepting" : "Rejecting"} {version.source} draft…
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
  row: VersionsTab_proposal$key;
  accepting: boolean;
  rejecting: boolean;
  busy: boolean;
  onAccept: (version: VersionIdentity) => void;
  onReject: (version: VersionIdentity) => void;
};

function PendingRow({
  row,
  accepting,
  rejecting,
  busy,
  onAccept,
  onReject,
}: Readonly<PendingRowProps>): ReactElement {
  const { id, source, matchType, confidenceScore, newValue } = useFragment(proposalFragment, row);
  const value = renderProposalValue(newValue);

  return (
    <li className="border-border flex items-start justify-between gap-4 rounded-sm border p-3">
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs uppercase tracking-wide">
          {source} · {matchType} · {Math.round(confidenceScore * 100).toString()}%
        </p>
        <p className="text-fg mt-1 break-words">{value}</p>
      </div>
      <div className="flex shrink-0 gap-2">
        <Button
          size="sm"
          onPress={() => {
            onAccept({ id, source });
          }}
          isPending={accepting}
          isDisabled={busy && !accepting}
        >
          Accept
        </Button>
        <Button
          size="sm"
          variant="outline"
          onPress={() => {
            onReject({ id, source });
          }}
          isPending={rejecting}
          isDisabled={busy && !rejecting}
        >
          Reject
        </Button>
      </div>
    </li>
  );
}

function canonicalValue(book: VersionsTab_book$data, field: string): string | null {
  switch (field) {
    case "title":
      return book.work?.title ?? null;
    case "description":
      return book.work?.description ?? null;
    case "language":
      return book.work?.language ?? null;
    case "isbn_10":
      return book.isbn10 ?? null;
    case "isbn_13":
      return book.isbn13 ?? null;
    case "publisher":
      return book.publisher ?? null;
    case "pub_date":
      return book.publicationDate ?? null;
    default:
      return null;
  }
}

function renderProposalValue(value: VersionsTab_proposal$data["newValue"]): string {
  switch (value.__typename) {
    case "MetadataProposalText":
      return value.text;
    case "MetadataProposalInteger":
      return String(value.integer);
    case "MetadataProposalStringList":
      return JSON.stringify(value.strings);
    case "MetadataProposalSeries":
      return JSON.stringify({ name: value.name, position: value.position });
    case "MetadataProposalContributors":
      return JSON.stringify(value.contributors.map(({ code, name }) => ({ code, name })));
    default:
      return "(null)";
  }
}

function formatError(err: unknown): string {
  if (err instanceof ApiError) return `${err.title}: ${err.detail}`;
  if (err instanceof Error) return err.message;
  return "Request failed.";
}
