/**
 * Versions tab body for the book detail page.
 *
 * Renders pending `metadata_versions` rows grouped by `field_name`, with
 * accept / reject / revert affordances per row and an "Edit field"
 * action that opens [`EditMetadataDialog`]. Mutations invalidate the
 * `["books", "detail", id]` cache slot so the canonical fields, the
 * pending list, and the version-count badge refetch in lockstep.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactElement } from "react";
import { toast } from "sonner";

import {
  ApiError,
  acceptVersion,
  rejectVersion,
  revertField,
  type BookDetail,
  type MetadataVersionRow,
} from "@/api";
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

interface VersionsTabProps {
  book: BookDetail;
}

/**
 * Top-level Versions tab. Owns the Edit dialog open state but delegates
 * each row to [`PendingRow`] so accept/reject can render without
 * shared state.
 */
export function VersionsTab({ book }: VersionsTabProps): ReactElement {
  const [editOpen, setEditOpen] = useState(false);
  const grouped = groupByField(book.metadata_versions);
  const fieldsWithDrafts = Object.keys(grouped).sort();
  const canonicalEditableFields = (
    ["title", "description", "language", "publisher", "pub_date", "isbn_10", "isbn_13"] as const
  ).map((name) => ({
    name,
    label: FIELD_LABEL[name] ?? name,
    canonical: canonicalValue(book, name),
  }));

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
          onClick={() => {
            setEditOpen(true);
          }}
        >
          Edit metadata
        </Button>
      </div>

      {fieldsWithDrafts.length > 0 ? (
        <div className="space-y-6">
          {fieldsWithDrafts.map((field) => (
            <FieldGroup
              key={field}
              manifestationId={book.id}
              field={field}
              canonical={canonicalValue(book, field)}
              rows={grouped[field] ?? []}
            />
          ))}
        </div>
      ) : null}

      <EditMetadataDialog
        manifestationId={book.id}
        open={editOpen}
        onOpenChange={setEditOpen}
        fields={canonicalEditableFields}
      />
    </div>
  );
}

interface FieldGroupProps {
  manifestationId: string;
  field: string;
  canonical: string | null;
  rows: MetadataVersionRow[];
}

/** One field's section: current canonical row + pending alternatives. */
function FieldGroup({ manifestationId, field, canonical, rows }: FieldGroupProps): ReactElement {
  const queryClient = useQueryClient();
  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: queryKeys.books.detail(manifestationId),
    });

  const revertMutation = useMutation({
    mutationFn: () => revertField(manifestationId, field, null),
    onSuccess: () => {
      void invalidate();
      toast.success(`Cleared ${FIELD_LABEL[field] ?? field}`);
    },
    onError: (err: unknown) => toast.error(formatError(err)),
  });

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
        {canonical !== null && field !== "title" ? (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              revertMutation.mutate();
            }}
            disabled={revertMutation.isPending}
          >
            Clear
          </Button>
        ) : null}
      </header>
      <dl className="space-y-2 text-sm">
        <CanonicalRow canonical={canonical} />
        {rows.map((row) => (
          <PendingRow key={row.id} manifestationId={manifestationId} field={field} row={row} />
        ))}
      </dl>
    </section>
  );
}

interface CanonicalRowProps {
  canonical: string | null;
}

function CanonicalRow({ canonical }: CanonicalRowProps): ReactElement {
  return (
    <div className="border-border bg-surface-1 flex items-start justify-between gap-4 rounded-sm border p-3">
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs uppercase tracking-wide">Current</p>
        <p className="text-fg mt-1 break-words">
          {canonical ?? <span className="italic">unset</span>}
        </p>
      </div>
      <Badge variant="default">canonical</Badge>
    </div>
  );
}

interface PendingRowProps {
  manifestationId: string;
  field: string;
  row: MetadataVersionRow;
}

function PendingRow({ manifestationId, field, row }: PendingRowProps): ReactElement {
  const queryClient = useQueryClient();
  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: queryKeys.books.detail(manifestationId),
    });

  const acceptMutation = useMutation({
    mutationFn: () => acceptVersion(manifestationId, row.id),
    onSuccess: () => {
      void invalidate();
      toast.success(`Accepted ${row.source} draft for ${FIELD_LABEL[field] ?? field}`);
    },
    onError: (err: unknown) => toast.error(formatError(err)),
  });
  const rejectMutation = useMutation({
    mutationFn: () => rejectVersion(manifestationId, row.id),
    onSuccess: () => {
      void invalidate();
      toast.success(`Rejected ${row.source} draft`);
    },
    onError: (err: unknown) => toast.error(formatError(err)),
  });

  const value = renderValue(row.new_value);
  const busy = acceptMutation.isPending || rejectMutation.isPending;

  return (
    <div className="border-border flex items-start justify-between gap-4 rounded-sm border p-3">
      <div className="min-w-0 flex-1">
        <p className="text-fg-muted text-xs uppercase tracking-wide">
          {row.source} · {row.match_type} · {Math.round(row.confidence_score * 100).toString()}%
        </p>
        <p className="text-fg mt-1 break-words">{value}</p>
      </div>
      <div className="flex shrink-0 gap-2">
        <Button
          size="sm"
          onClick={() => {
            acceptMutation.mutate();
          }}
          disabled={busy}
        >
          Accept
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            rejectMutation.mutate();
          }}
          disabled={busy}
        >
          Reject
        </Button>
      </div>
    </div>
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
    case "pub_date":
      // Not currently surfaced on `BookDetail`; render as unknown so
      // the operator sees the pending value without confusion.
      return null;
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
