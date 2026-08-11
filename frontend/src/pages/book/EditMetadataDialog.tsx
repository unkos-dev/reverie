/**
 * Manual metadata edit sheet.
 *
 * Mounted as a controlled shadcn `<Sheet>` from [`VersionsTab`]. Each
 * field tracks a `touched` flag; only touched keys land in the
 * `PATCH /api/v1/books/{id}/metadata` body so the RFC 7396 sparse-update
 * semantics on the server see exactly the operator's intent.
 *
 * Untouched canonical values seed the inputs as a convenience — the
 * operator can clear an input to mark it for null (canonical column
 * cleared) or type a new value. The submit handler distinguishes the
 * two: a touched field with an empty input emits `null`; a touched
 * field with a non-empty input emits the string value.
 *
 * On open, the form fetches `GET /api/v1/books/{id}/metadata` itself
 * rather than seeding from the list/detail cache the caller may be
 * holding: only that response's `ETag` (captured by `apiFetch`) is
 * guaranteed current for the submit's `If-Match`, so a stale cached
 * canonical value would both seed the form wrong and risk a spurious 412.
 *
 * A confirmation `<AlertDialog>` interposes when the change would
 * clear an already-populated field, mirroring the per-row Clear
 * affordance on [`VersionsTab`].
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactElement } from "react";
import { toast } from "sonner";

import {
  ApiError,
  UpdateBookMetadataFieldsSchema,
  getBookMetadata,
  isIfMatchMismatch,
  updateBookMetadata,
  type BookMetadata,
  type UpdateBookMetadataFields,
} from "@/api";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Textarea } from "@/components/ui/textarea";
import { queryKeys } from "@/lib/query/keys";

/** One editable field on the edit form. Its canonical value seeds from a
 *  fresh `GET`, not from a caller-supplied snapshot. */
export interface EditableField {
  /** Wire name — must match a key in [`UpdateBookMetadataFields`] and a
   *  scalar field on [`BookMetadata`]. */
  name: keyof UpdateBookMetadataFields;
  /** Display label shown above the input. */
  label: string;
}

interface EditMetadataDialogProps {
  manifestationId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fields: readonly EditableField[];
}

/** Reads one editable field's canonical value off a fresh `BookMetadata`
 *  read. Limited to the scalar string fields [`VersionsTab`] currently
 *  offers; an unmapped field (e.g. `contributors`) has no single string
 *  representation and seeds empty. */
function canonicalValue(
  metadata: BookMetadata,
  field: keyof UpdateBookMetadataFields,
): string | null {
  switch (field) {
    case "title":
      return metadata.title;
    case "subtitle":
      return metadata.subtitle;
    case "description":
      return metadata.description;
    case "language":
      return metadata.language;
    case "publisher":
      return metadata.publisher;
    case "pub_date":
      return metadata.pub_date;
    case "isbn_10":
      return metadata.isbn_10;
    case "isbn_13":
      return metadata.isbn_13;
    default:
      return null;
  }
}

type FieldState = Partial<
  Record<keyof UpdateBookMetadataFields, { value: string; touched: boolean }>
>;

const FIELDS_AS_TEXTAREA: ReadonlySet<string> = new Set(["description"]);

/**
 * The sheet wrapper. State lives in [`EditMetadataForm`] which mounts
 * fresh each time `open` flips true (via the `key` prop) — that side-
 * steps the need for an effect to reset form state when re-opening.
 */
export function EditMetadataDialog({
  manifestationId,
  open,
  onOpenChange,
  fields,
}: EditMetadataDialogProps): ReactElement {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="flex w-full flex-col sm:max-w-lg">
        <SheetHeader>
          <SheetTitle>Edit metadata</SheetTitle>
          <SheetDescription>
            Empty an input to clear that field on save. Untouched fields stay as-is.
          </SheetDescription>
        </SheetHeader>
        {open ? (
          <EditMetadataForm
            key={manifestationId}
            manifestationId={manifestationId}
            fields={fields}
            onDone={() => {
              onOpenChange(false);
            }}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  );
}

interface EditMetadataFormProps {
  manifestationId: string;
  fields: readonly EditableField[];
  onDone: () => void;
}

/**
 * Fetches a fresh metadata read before rendering anything editable: the
 * form's seed values and the eventual PATCH's `If-Match` both depend on
 * this response, not on whatever the caller happens to have cached.
 */
function EditMetadataForm({
  manifestationId,
  fields,
  onDone,
}: EditMetadataFormProps): ReactElement {
  const metadataQuery = useQuery({
    queryKey: queryKeys.books.metadata(manifestationId),
    queryFn: ({ signal }) => getBookMetadata(manifestationId, signal),
  });

  if (metadataQuery.isPending) {
    return (
      <div className="text-fg-muted flex flex-1 items-center justify-center px-4 py-8 text-sm">
        Loading current metadata…
      </div>
    );
  }
  if (metadataQuery.isError) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-8">
        <p className="text-fg-muted text-sm">Couldn&apos;t load the latest metadata.</p>
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            void metadataQuery.refetch();
          }}
        >
          Retry
        </Button>
      </div>
    );
  }

  return (
    <EditMetadataFields
      manifestationId={manifestationId}
      fields={fields}
      metadata={metadataQuery.data}
      onDone={onDone}
    />
  );
}

interface EditMetadataFieldsProps {
  manifestationId: string;
  fields: readonly EditableField[];
  metadata: BookMetadata;
  onDone: () => void;
}

function EditMetadataFields({
  manifestationId,
  fields,
  metadata,
  onDone,
}: EditMetadataFieldsProps): ReactElement {
  const queryClient = useQueryClient();
  const [state, setState] = useState<FieldState>(() => buildInitialState(fields, metadata));
  const [pendingClearConfirm, setPendingClearConfirm] = useState<UpdateBookMetadataFields | null>(
    null,
  );

  const mutation = useMutation({
    mutationFn: (body: UpdateBookMetadataFields) => updateBookMetadata(manifestationId, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.books.detail(manifestationId),
      });
      toast.success("Metadata updated.");
      onDone();
    },
    onError: (err: unknown) => {
      console.error("[EditMetadataDialog.updateBookMetadata] mutation failed", err);
      if (isIfMatchMismatch(err)) {
        // The 412 already carries the current ETag (captured by apiFetch),
        // so invalidating both caches is enough to resync on reopen; no
        // merge UI. The sheet closes because its fields seed fresh from
        // `getBookMetadata` on the next open, not from either cache here.
        void queryClient.invalidateQueries({
          queryKey: queryKeys.books.detail(manifestationId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.books.metadata(manifestationId),
        });
        toast.error("This book's metadata changed elsewhere.", {
          action: { label: "Reload latest", onClick: onDone },
        });
        return;
      }
      toast.error(formatError(err));
    },
  });

  function buildBody(): { body: UpdateBookMetadataFields; clears: string[] } {
    const raw: Record<string, string | null> = {};
    const clears: string[] = [];
    for (const field of fields) {
      const slot = state[field.name];
      if (!slot || !slot.touched) continue;
      const trimmed = slot.value.trim();
      if (trimmed.length === 0) {
        raw[field.name] = null;
        if (canonicalValue(metadata, field.name) !== null) clears.push(field.label);
      } else {
        raw[field.name] = trimmed;
      }
    }
    // Boundary parse — form inputs must be schema-validated before
    // crossing into the API client. A parse
    // failure here is a programmer error (mismatch between the form
    // and `UpdateBookMetadataFieldsSchema`), not a user-input error.
    const body = UpdateBookMetadataFieldsSchema.parse(raw);
    return { body, clears };
  }

  return (
    <>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          let body: UpdateBookMetadataFields;
          let clears: string[];
          try {
            ({ body, clears } = buildBody());
          } catch (err) {
            console.error("[EditMetadataDialog.buildBody] schema parse failed", err);
            toast.error("Form has an invalid value. Refresh and try again.");
            return;
          }
          if (Object.keys(body).length === 0) {
            toast.error("No fields touched — nothing to save.");
            return;
          }
          if (clears.length > 0) {
            setPendingClearConfirm(body);
            return;
          }
          mutation.mutate(body);
        }}
        className="flex flex-1 flex-col overflow-y-auto px-4"
      >
        <div className="space-y-4 py-2">
          {fields.map((field) => (
            <FieldRow
              key={field.name}
              field={field}
              hasCanonicalValue={canonicalValue(metadata, field.name) !== null}
              state={state[field.name] ?? { value: "", touched: false }}
              onChange={(value) => {
                setState((prev) => ({
                  ...prev,
                  [field.name]: { value, touched: true },
                }));
              }}
            />
          ))}
        </div>
        <SheetFooter className="mt-auto pt-4">
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              onDone();
            }}
            disabled={mutation.isPending}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={mutation.isPending}>
            Save
          </Button>
        </SheetFooter>
      </form>

      <AlertDialog
        open={pendingClearConfirm !== null}
        onOpenChange={(o) => {
          if (!o) setPendingClearConfirm(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Clear populated field?</AlertDialogTitle>
            <AlertDialogDescription>
              You&apos;re clearing a field that already has a value. The canonical column will be
              set to NULL and an audit row recorded. Continue?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              onClick={() => {
                setPendingClearConfirm(null);
              }}
            >
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingClearConfirm) {
                  mutation.mutate(pendingClearConfirm);
                  setPendingClearConfirm(null);
                }
              }}
            >
              Clear and save
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

interface FieldRowProps {
  field: EditableField;
  hasCanonicalValue: boolean;
  state: { value: string; touched: boolean };
  onChange: (value: string) => void;
}

function FieldRow({ field, hasCanonicalValue, state, onChange }: FieldRowProps): ReactElement {
  const id = `edit-${field.name}`;
  const useTextarea = FIELDS_AS_TEXTAREA.has(field.name);
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{field.label}</Label>
      {useTextarea ? (
        <Textarea
          id={id}
          value={state.value}
          onChange={(e) => {
            onChange(e.target.value);
          }}
          rows={4}
        />
      ) : (
        <Input
          id={id}
          value={state.value}
          onChange={(e) => {
            onChange(e.target.value);
          }}
        />
      )}
      {hasCanonicalValue ? null : <p className="text-fg-muted text-xs">No canonical value yet.</p>}
    </div>
  );
}

function buildInitialState(fields: readonly EditableField[], metadata: BookMetadata): FieldState {
  const out: FieldState = {};
  for (const field of fields) {
    out[field.name] = { value: canonicalValue(metadata, field.name) ?? "", touched: false };
  }
  return out;
}

function formatError(err: unknown): string {
  if (err instanceof ApiError) return `${err.title}: ${err.detail}`;
  if (err instanceof Error) return err.message;
  return "Request failed.";
}
