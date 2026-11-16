/** Manual metadata editing retains one matched raw snapshot through save recovery. */
import { useActionState, useEffect, useId, useRef, useState, type ReactElement } from "react";
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

import { ApiError, UpdateBookMetadataFieldsSchema, type UpdateBookMetadataFields } from "@/api";
import {
  GraphQLExecutionError,
  editingFailureAction,
  encodeGraphQLId,
  executeEditingOnce,
} from "@/api/graphql";
import { useRelayEnvironment } from "react-relay";
import { commitLocalUpdate } from "relay-runtime";
import {
  mutationsFormQuery,
  mutationsSaveFormMutation,
  mutationsResolveFormQuery,
} from "@/lib/relay/mutations";
import type { mutationsFormQuery as FormQuery } from "@/lib/relay/__generated__/mutationsFormQuery.graphql";
import type {
  mutationsSaveFormMutation as SaveForm,
  MetadataPatch,
} from "@/lib/relay/__generated__/mutationsSaveFormMutation.graphql";
import type { mutationsResolveFormQuery as ResolveForm } from "@/lib/relay/__generated__/mutationsResolveFormQuery.graphql";
type FormSnapshot = SaveForm["response"]["updateMetadata"]["metadata"];
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
  metadata: FormSnapshot,
  field: keyof UpdateBookMetadataFields,
): string | null {
  switch (field) {
    case "title":
      return metadata.title;
    case "subtitle":
      return metadata.subtitle ?? null;
    case "description":
      return metadata.description ?? null;
    case "language":
      return metadata.language ?? null;
    case "publisher":
      return metadata.publisher ?? null;
    case "pub_date":
      return metadata.publicationDate ?? null;
    case "isbn_10":
      return metadata.isbn10 ?? null;
    case "isbn_13":
      return metadata.isbn13 ?? null;
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
  const environment = useRelayEnvironment();
  const [reloadVersion, setReloadVersion] = useState(0);
  const [snapshot, setSnapshot] = useState<{ metadata: FormSnapshot; version: number }>();
  const [error, setError] = useState<Error | null>(null);
  const [reloading, setReloading] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    void executeEditingOnce<FormQuery>(
      environment,
      mutationsFormQuery,
      { id: encodeGraphQLId("Manifestation", manifestationId) },
      controller.signal,
    )
      .then((response) => {
        if (response === undefined || controller.signal.aborted) return;
        if (response.manifestation == null) setError(new Error("Book not found."));
        else {
          setSnapshot({ metadata: response.manifestation.metadata, version: reloadVersion });
          setError(null);
        }
        setReloading(false);
      })
      .catch((failure: unknown) => {
        if (!controller.signal.aborted) {
          setError(failure instanceof Error ? failure : new Error("Request failed."));
          setReloading(false);
        }
      });
    return () => {
      controller.abort();
    };
  }, [environment, manifestationId, reloadVersion]);
  if (snapshot === undefined && error === null)
    return (
      <div className="text-fg-muted flex flex-1 items-center justify-center px-4 py-8 text-sm">
        Loading current metadata…
      </div>
    );
  if (snapshot === undefined)
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-8">
        <p className="text-fg-muted text-sm">Couldn&apos;t load the latest metadata.</p>
        <Button
          type="button"
          variant="outline"
          onPress={() => {
            setError(null);
            setReloadVersion((value) => value + 1);
          }}
        >
          Retry
        </Button>
      </div>
    );
  return (
    <EditMetadataFields
      key={snapshot.version}
      onReload={() => {
        setReloading(true);
        setReloadVersion((value) => value + 1);
      }}
      reloading={reloading}
      reloadError={error !== null}
      manifestationId={manifestationId}
      fields={fields}
      metadata={snapshot.metadata}
      onDone={onDone}
    />
  );
}

type EditMetadataFieldsProps = {
  onReload: () => void;
  reloading: boolean;
  reloadError: boolean;
  manifestationId: string;
  fields: readonly EditableField[];
  metadata: FormSnapshot;
  onDone: () => void;
};

function EditMetadataFields({
  onReload,
  reloading,
  reloadError,
  manifestationId,
  fields,
  metadata: incoming,
  onDone,
}: Readonly<EditMetadataFieldsProps>): ReactElement {
  const environment = useRelayEnvironment();
  const controller = useRef<AbortController | null>(null);
  useEffect(() => {
    const lifetime = new AbortController();
    controller.current = lifetime;
    return () => {
      lifetime.abort();
    };
  }, []);
  const [baseline] = useState(incoming);
  const metadata = baseline;
  const attempt = useRef<{ body: MetadataPatch; requestId: string } | null>(null);
  const touched = useRef(new Set<keyof UpdateBookMetadataFields>());
  const [pendingClearConfirm, setPendingClearConfirm] = useState(false);
  const formId = useId();
  function reconcile(snapshot: FormSnapshot): void {
    commitLocalUpdate(environment, (store) => {
      const work = store.get(snapshot.workId);
      const edition = store.get(snapshot.manifestationId);
      for (const key of ["title", "subtitle", "description", "language", "contentRating"] as const)
        work?.setValue(snapshot[key], key);
      for (const key of [
        "isbn10",
        "isbn13",
        "isbnMismatch",
        "publisher",
        "publicationDate",
        "pages",
      ] as const)
        edition?.setValue(snapshot[key], key);
      work?.invalidateRecord();
      edition?.invalidateRecord();
    });
  }
  function buildBody(values: Record<string, string>): MetadataPatch {
    const raw: Record<string, string | null> = {};
    for (const field of fields) {
      if (!touched.current.has(field.name)) continue;
      const trimmed = (values[field.name] ?? "").trim();
      if (trimmed.length === 0) {
        raw[field.name] = null;
      } else {
        raw[field.name] = trimmed;
      }
    }
    const body = UpdateBookMetadataFieldsSchema.parse(raw);
    return {
      ...("title" in body ? { title: body.title } : {}),
      ...("subtitle" in body ? { subtitle: body.subtitle } : {}),
      ...("description" in body ? { description: body.description } : {}),
      ...("language" in body ? { language: body.language } : {}),
      ...("publisher" in body ? { publisher: body.publisher } : {}),
      ...("pub_date" in body ? { publicationDate: body.pub_date } : {}),
      ...("isbn_10" in body ? { isbn10: body.isbn_10 } : {}),
      ...("isbn_13" in body ? { isbn13: body.isbn_13 } : {}),
    };
  }

  const [state, action, pending] = useActionState<
    {
      values: Partial<Record<string, string>>;
      error: string | null;
      errors?: Record<string, string>;
      recovery?: ReturnType<typeof editingFailureAction>;
    },
    FormData
  >(
    async (_previous, data) => {
      const values = z.record(z.string(), z.string()).parse(Object.fromEntries(data));
      let body: MetadataPatch;
      try {
        body = buildBody(values);
      } catch (err) {
        console.error("[EditMetadataDialog.buildBody] schema parse failed", err);
        const error = "Form has an invalid value. Refresh and try again.";
        return { values, error };
      }
      if (Object.keys(body).length === 0) {
        const error = "No fields touched — nothing to save.";
        return { values, error };
      }
      setPendingClearConfirm(false);
      try {
        const previous = attempt.current;
        const unchanged =
          previous !== null && JSON.stringify(previous.body) === JSON.stringify(body);
        if (unchanged && _previous.recovery !== undefined && _previous.recovery !== "retry")
          return { ..._previous, values };
        const requestId = unchanged ? previous.requestId : crypto.randomUUID();
        attempt.current = { body, requestId };
        const variables = {
          id: encodeGraphQLId("Manifestation", manifestationId),
          patch: body,
          baseline: baseline.etag,
          requestId,
        };
        let saved: FormSnapshot | undefined;
        let failure: unknown;
        try {
          saved = (
            await executeEditingOnce<SaveForm>(environment, mutationsSaveFormMutation, variables)
          )?.updateMetadata.metadata;
        } catch (error) {
          failure = error;
        }
        if (failure !== undefined && ["retry", "reload"].includes(editingFailureAction(failure))) {
          try {
            const resolution = await executeEditingOnce<ResolveForm>(
              environment,
              mutationsResolveFormQuery,
              { id: variables.id, requestId },
            );
            if (resolution?.metadataOperation.outcome === "saved")
              saved = resolution.metadataOperation.metadata;
          } catch (error) {
            console.error("[EditMetadataDialog] save resolution failed", error);
          }
        }
        if (saved !== undefined) {
          reconcile(saved);
          const confirmed = await executeEditingOnce<FormQuery>(environment, mutationsFormQuery, {
            id: variables.id,
          });
          if (confirmed?.manifestation != null) {
            reconcile(confirmed.manifestation.metadata);
            if (!controller.current?.signal.aborted) onDone();
          }
          return { values, error: null };
        }
        const errors =
          failure instanceof GraphQLExecutionError
            ? failure.errors.at(0)?.extensions?.errors
            : undefined;
        return {
          values,
          errors,
          error: failure === undefined ? null : formatError(failure),
          recovery: editingFailureAction(failure),
        };
      } catch (err) {
        const errors = err instanceof ApiError ? err.errors : undefined;
        return {
          values,
          error: Object.keys(errors ?? {}).length > 0 ? null : formatError(err),
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
        onSubmit={(event) => {
          const submitter =
            event.nativeEvent instanceof SubmitEvent ? event.nativeEvent.submitter : null;
          const data = new FormData(event.currentTarget, submitter);
          if (data.get("confirm_clear") === "true") return;
          const clears = fields.some((field) => {
            const value = data.get(field.name);
            return (
              touched.current.has(field.name) &&
              canonicalValue(metadata, field.name) !== null &&
              typeof value === "string" &&
              value.trim().length === 0
            );
          });
          if (clears) {
            event.preventDefault();
            setPendingClearConfirm(true);
          }
        }}
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
              isReadOnly={pending || state.recovery === "retry"}
              onChange={() => {
                touched.current.add(field.name);
              }}
            />
          ))}
          {metadata.isbnMismatch ? (
            <p className="text-fg-muted text-sm">
              These ISBN-10 and ISBN-13 values do not correspond. Check both values.
            </p>
          ) : null}
        </div>
        {state.error && !pending ? <FieldError>{state.error}</FieldError> : null}
        {state.recovery === "reload" ? (
          <div className="space-y-2 py-2">
            <FieldError>
              Reload the latest metadata before another editing decision. Your entered edits are
              retained.
            </FieldError>
            <p className="text-fg-muted text-sm">
              Reloading replaces your entered edits with the latest metadata.
            </p>
            <Button type="button" variant="outline" isPending={reloading} onPress={onReload}>
              Discard edits and reload
            </Button>
            {reloadError ? (
              <FieldError>
                Could not reload metadata. Your entered edits are still available. Try again.
              </FieldError>
            ) : null}
          </div>
        ) : null}
        <SheetFooter className="mt-auto pt-4">
          <Button
            type="button"
            variant="outline"
            onPress={() => {
              onDone();
            }}
            isDisabled={pending}
          >
            Cancel
          </Button>
          <Button type="submit" isPending={pending}>
            {state.recovery === "retry" ? <>{pending ? "Retrying…" : "Retry"}</> : "Save"}
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
            isDisabled={pending}
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
  isReadOnly: boolean;
  onChange: () => void;
};

function FieldRow({
  field,
  hasCanonicalValue,
  defaultValue,
  isReadOnly,
  onChange,
}: Readonly<FieldRowProps>): ReactElement {
  const useTextarea = FIELDS_AS_TEXTAREA.has(field.name);
  return (
    <TextField
      name={field.name}
      defaultValue={defaultValue}
      isReadOnly={isReadOnly}
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
  if (err instanceof ApiError) return [err.title, err.detail].filter(Boolean).join(": ");
  if (err instanceof Error) return err.message;
  return "Request failed.";
}
