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
import { useActionState, useId, useRef, useState, type ReactElement } from "react";
import { flushSync } from "react-dom";
import { z } from "zod";
import {
  Form,
  FieldError as AriaFieldError,
  TextField,
  Text as AriaText,
  Label as AriaLabel,
  Input as AriaInput,
  TextArea as AriaTextArea,
} from "react-aria-components";
import { Popover, PopoverContent } from "@/components/ui/popover";
import { FieldError } from "@/components/ui/field";
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
export type EditableField = {
  /** Wire name — must match a key in [`UpdateBookMetadataFields`] and a
   *  scalar field on [`BookMetadata`]. */
  name: keyof UpdateBookMetadataFields;
  /** Display label shown above the input. */
  label: string;
};

type EditMetadataDialogProps = {
  manifestationId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fields: readonly EditableField[];
};

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
}: Readonly<EditMetadataDialogProps>): ReactElement {
  return (
    <SheetContent
      isOpen={open}
      onOpenChange={onOpenChange}
      side="right"
      className="flex w-full flex-col sm:max-w-lg"
    >
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
  );
}

type EditMetadataFormProps = {
  manifestationId: string;
  fields: readonly EditableField[];
  onDone: () => void;
};

/**
 * Fetches a fresh metadata read before rendering anything editable: the
 * form's seed values and the eventual PATCH's `If-Match` both depend on
 * this response, not on whatever the caller happens to have cached.
 */
function EditMetadataForm({
  manifestationId,
  fields,
  onDone,
}: Readonly<EditMetadataFormProps>): ReactElement {
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
          onPress={() => {
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

type EditMetadataFieldsProps = {
  manifestationId: string;
  fields: readonly EditableField[];
  metadata: BookMetadata;
  onDone: () => void;
};

function EditMetadataFields({
  manifestationId,
  fields,
  metadata,
  onDone,
}: Readonly<EditMetadataFieldsProps>): ReactElement {
  const queryClient = useQueryClient();
  const touched = useRef(new Set<keyof UpdateBookMetadataFields>());
  const [pendingClearConfirm, setPendingClearConfirm] = useState(false);
  const formId = useId();
  const mutation = useMutation({
    mutationFn: (body: UpdateBookMetadataFields) => updateBookMetadata(manifestationId, body),
    onSuccess: async () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.books.detail(manifestationId),
      });
      await queryClient.invalidateQueries({
        queryKey: queryKeys.books.metadata(manifestationId),
      });
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
      }
    },
  });

  function buildBody(values: Record<string, string>): {
    body: UpdateBookMetadataFields;
    clears: string[];
  } {
    const raw: Record<string, string | null> = {};
    const clears: string[] = [];
    for (const field of fields) {
      if (!touched.current.has(field.name)) continue;
      const trimmed = (values[field.name] ?? "").trim();
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

  const [state, action, pending] = useActionState<
    {
      values: Partial<Record<string, string>>;
      error: string | null;
      errors?: Record<string, string>;
    },
    FormData
  >(
    async (_previous, data) => {
      const values = z.record(z.string(), z.string()).parse(Object.fromEntries(data));
      let body: UpdateBookMetadataFields;
      let clears: string[];
      try {
        ({ body, clears } = buildBody(values));
      } catch (err) {
        console.error("[EditMetadataDialog.buildBody] schema parse failed", err);
        const error = "Form has an invalid value. Refresh and try again.";
        return { values, error };
      }
      if (Object.keys(body).length === 0) {
        const error = "No fields touched — nothing to save.";
        return { values, error };
      }
      if (clears.length > 0 && data.get("confirm_clear") !== "true") {
        // Mount before the parent modal's observer can make the confirmation inert.
        flushSync(() => {
          setPendingClearConfirm(true);
        });
        return { values, error: null };
      }
      setPendingClearConfirm(false);
      try {
        await mutation.mutateAsync(body);
        return { values, error: null };
      } catch (err) {
        const errors = err instanceof ApiError ? err.errors : undefined;
        return {
          values,
          error:
            isIfMatchMismatch(err) || Object.keys(errors ?? {}).length > 0
              ? null
              : formatError(err),
          errors,
        };
      }
    },
    { values: {}, error: null },
  );

  return (
    <>
      <Form
        id={formId}
        action={action}
        validationErrors={state.errors ?? {}}
        className="flex flex-1 flex-col overflow-y-auto px-4"
      >
        <div className="space-y-4 py-2">
          {fields.map((field) => (
            <FieldRow
              key={field.name}
              field={field}
              hasCanonicalValue={canonicalValue(metadata, field.name) !== null}
              defaultValue={state.values[field.name] ?? canonicalValue(metadata, field.name) ?? ""}
              onChange={() => {
                touched.current.add(field.name);
              }}
            />
          ))}
        </div>
        {state.error ? <FieldError>{state.error}</FieldError> : null}
        <SheetFooter className="mt-auto pt-4">
          <Button
            type="button"
            variant="outline"
            onPress={() => {
              onDone();
            }}
            isDisabled={mutation.isPending}
          >
            Cancel
          </Button>
          <Button type="submit" isPending={pending}>
            Save
          </Button>
        </SheetFooter>
      </Form>

      <AlertDialogContent
        isOpen={pendingClearConfirm}
        onOpenChange={(o) => {
          if (!o) setPendingClearConfirm(false);
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Clear populated field?</AlertDialogTitle>
          <AlertDialogDescription>
            You&apos;re clearing a field that already has a value. The canonical column will be set
            to NULL and an audit row recorded. Continue?
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel
            isDisabled={mutation.isPending}
            onPress={() => {
              setPendingClearConfirm(false);
            }}
          >
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            type="submit"
            form={formId}
            name="confirm_clear"
            value="true"
            isPending={pending}
          >
            Clear and save
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </>
  );
}

type FieldRowProps = {
  field: EditableField;
  hasCanonicalValue: boolean;
  defaultValue: string;
  onChange: () => void;
};

function FieldRow({
  field,
  hasCanonicalValue,
  defaultValue,
  onChange,
}: Readonly<FieldRowProps>): ReactElement {
  const useTextarea = FIELDS_AS_TEXTAREA.has(field.name);
  return (
    <TextField
      name={field.name}
      defaultValue={defaultValue}
      onChange={onChange}
      className="space-y-1.5"
    >
      <AriaLabel render={(props) => <Label {...props} />}>{field.label}</AriaLabel>
      {useTextarea ? (
        <AriaTextArea render={(props) => <Textarea {...props} />} rows={4} />
      ) : (
        <AriaInput render={(props) => <Input {...props} />} />
      )}
      {field.name === "pub_date" && (
        <>
          <AriaText slot="description" className="text-fg-muted block text-xs">
            Use YYYY-MM-DD or YYYY. Leave empty if unknown.
          </AriaText>
          <Popover>
            <Button type="button" variant="ghost" size="xs">
              Accepted formats
            </Button>
            <PopoverContent
              aria-label="Accepted publication date formats"
              className="max-w-[calc(100vw-2rem)] space-y-3"
            >
              <ul className="space-y-1 break-all font-mono text-xs">
                <li>YYYY</li>
                <li>YYYY-MM</li>
                <li>YYYY-MM-DD</li>
                <li>YYYY-MM-DDThh:mmTZD</li>
                <li>YYYY-MM-DDThh:mm:ssTZD</li>
                <li>YYYY-MM-DDThh:mm:ss.sTZD</li>
              </ul>
              <p>
                TZD is Z or an offset such as +01:00. Fractional seconds can contain one or more
                digits.
              </p>
              <a
                href="https://www.w3.org/TR/NOTE-datetime#Formats"
                target="_blank"
                rel="noopener noreferrer"
                className="underline underline-offset-4"
              >
                Format reference
              </a>
            </PopoverContent>
          </Popover>
        </>
      )}
      {field.name !== "pub_date" && !hasCanonicalValue && (
        <p className="text-fg-muted text-xs">No canonical value yet.</p>
      )}
      <AriaFieldError className="text-fg block text-sm font-normal" />
    </TextField>
  );
}

function formatError(err: unknown): string {
  if (err instanceof ApiError) return `${err.title}: ${err.detail}`;
  if (err instanceof Error) return err.message;
  return "Request failed.";
}
