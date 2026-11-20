/** Manual metadata editing keeps the revision it opened with as its baseline through save recovery. */
import {
  Suspense,
  useActionState,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactElement,
} from "react";
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
import { ErrorBoundary } from "react-error-boundary";
import {
  fetchQuery,
  useFragment,
  useMutation,
  usePreloadedQuery,
  useRelayEnvironment,
  type PreloadedQuery,
} from "react-relay";
import { commitLocalUpdate, type FetchPolicy } from "relay-runtime";
import { Popover, PopoverContent } from "@/components/ui/popover";
import { FieldError } from "@/components/ui/field";

import { ApiError, UpdateBookMetadataFieldsSchema, type UpdateBookMetadataFields } from "@/api";
import { MetadataRefusal, editingFailureAction } from "@/api/graphql";
import {
  mutationsFormQuery,
  mutationsFormSnapshot,
  mutationsSaveFormMutation,
  mutationsResolveFormQuery,
} from "@/lib/relay/mutations";
import type { mutationsFormQuery as FormQuery } from "@/lib/relay/__generated__/mutationsFormQuery.graphql";
import type {
  mutationsFormSnapshot$data as FormSnapshot,
  mutationsFormSnapshot$key,
} from "@/lib/relay/__generated__/mutationsFormSnapshot.graphql";
import type {
  mutationsSaveFormMutation as SaveForm,
  MetadataPatch,
  UpdateMetadataInput,
} from "@/lib/relay/__generated__/mutationsSaveFormMutation.graphql";
import type { mutationsResolveFormQuery as ResolveForm } from "@/lib/relay/__generated__/mutationsResolveFormQuery.graphql";
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

/** One editable field on the edit form, seeded from the metadata revision the form opened with. */
export type EditableField = {
  /** Wire name — must match a key in [`UpdateBookMetadataFields`] and a
   *  scalar field on [`BookMetadata`]. */
  name: keyof UpdateBookMetadataFields;
  /** Display label shown above the input. */
  label: string;
};

type EditMetadataDialogProps = {
  /** The form read the opening event handler started. */
  queryRef: PreloadedQuery<FormQuery> | null | undefined;
  /** Starts the form read again with the given fetch policy. */
  load: (fetchPolicy: FetchPolicy) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fields: readonly EditableField[];
};

/** Reads one editable field's canonical value off the opening revision. Limited to the scalar string
 *  fields [`VersionsTab`] currently offers; an unmapped field (e.g. `contributors`) has no single string
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
 * The sheet wrapper. The form mounts fresh each time `open` flips true, so each opening starts from the
 * revision its own read returned.
 */
export function EditMetadataDialog({
  queryRef,
  load,
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
      {open && queryRef != null ? (
        <ErrorBoundary
          resetKeys={[queryRef]}
          fallback={
            <LoadFailure
              onRetry={() => {
                load("network-only");
              }}
            />
          }
        >
          <Suspense
            fallback={
              <div className="text-fg-muted flex flex-1 items-center justify-center px-4 py-8 text-sm">
                Loading current metadata…
              </div>
            }
          >
            <EditMetadataForm
              queryRef={queryRef}
              fields={fields}
              onDone={() => {
                onOpenChange(false);
              }}
            />
          </Suspense>
        </ErrorBoundary>
      ) : null}
    </SheetContent>
  );
}

function LoadFailure({ onRetry }: Readonly<{ onRetry: () => void }>): ReactElement {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-8">
      <p className="text-fg-muted text-sm">Couldn&apos;t load the latest metadata.</p>
      <Button type="button" variant="outline" onPress={onRetry}>
        Retry
      </Button>
    </div>
  );
}

type EditMetadataFormProps = {
  queryRef: PreloadedQuery<FormQuery>;
  fields: readonly EditableField[];
  onDone: () => void;
};

/** Owns the explicit Reload, which replaces the opening revision only once a fresh read has arrived. */
function EditMetadataForm({
  queryRef,
  fields,
  onDone,
}: Readonly<EditMetadataFormProps>): ReactElement {
  const environment = useRelayEnvironment();
  const data = usePreloadedQuery<FormQuery>(mutationsFormQuery, queryRef);
  const [opening, setOpening] = useState(0);
  const [reloading, setReloading] = useState(false);
  const [reloadError, setReloadError] = useState(false);
  const manifestation = data.manifestation;
  const [openedWithBook] = useState(manifestation != null);
  if (manifestation == null && !openedWithBook) throw new Error("Book not found.");
  const id = queryRef.variables.id;
  const reload = (): void => {
    if (reloading) return;
    setReloading(true);
    let found = false;
    fetchQuery<FormQuery>(
      environment,
      mutationsFormQuery,
      { id },
      { fetchPolicy: "network-only" },
    ).subscribe({
      next: (response) => {
        found = response.manifestation != null;
      },
      complete: () => {
        setReloading(false);
        setReloadError(!found);
        if (found) setOpening((value) => value + 1);
      },
      error: () => {
        setReloading(false);
        setReloadError(true);
      },
    });
  };
  return (
    <EditMetadataFields
      key={opening}
      onReload={reload}
      reloading={reloading}
      reloadError={reloadError}
      manifestationId={id}
      fields={fields}
      metadata={manifestation?.metadata ?? null}
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
  metadata: mutationsFormSnapshot$key | null;
  onDone: () => void;
};

function EditMetadataFields({
  onReload,
  reloading,
  reloadError,
  manifestationId,
  fields,
  metadata: metadataKey,
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
  const finish = (): void => {
    if (!controller.current?.signal.aborted) onDone();
  };
  const current = useFragment(mutationsFormSnapshot, metadataKey);
  // A later store merge must not advance this edit's write precondition.
  const [metadata] = useState(() => {
    if (current == null) throw new Error("Book not found.");
    return current;
  });
  const [commitSave] = useMutation<SaveForm>(mutationsSaveFormMutation);
  const attempt = useRef<{ body: MetadataPatch; requestId: string } | null>(null);
  const touched = useRef(new Set<keyof UpdateBookMetadataFields>());
  const [pendingClearConfirm, setPendingClearConfirm] = useState(false);
  const formId = useId();
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

  function save(input: UpdateMetadataInput): Promise<SaveForm["response"]["updateMetadata"]> {
    return new Promise((resolve, reject) => {
      commitSave({
        variables: { input },
        onCompleted: (response) => {
          resolve(response.updateMetadata);
        },
        onError: reject,
      });
    });
  }

  async function resolvedSaved(requestId: string): Promise<boolean> {
    try {
      const resolution = await fetchQuery<ResolveForm>(
        environment,
        mutationsResolveFormQuery,
        { id: manifestationId, requestId },
        { fetchPolicy: "network-only" },
      ).toPromise();
      if (resolution?.metadataOperation.outcome !== "saved") return false;
      commitLocalUpdate(environment, (store) => {
        store.get(manifestationId)?.invalidateRecord();
      });
      return true;
    } catch (error) {
      console.error("[EditMetadataDialog] save resolution failed", error);
      return false;
    }
  }

  const [state, action, pending] = useActionState<
    {
      values: Partial<Record<string, string>>;
      error: string | null;
      errors?: Readonly<Record<string, string>>;
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
      const previous = attempt.current;
      const unchanged = previous !== null && JSON.stringify(previous.body) === JSON.stringify(body);
      if (unchanged && _previous.recovery !== undefined && _previous.recovery !== "retry")
        return { ..._previous, values };
      const requestId = unchanged ? previous.requestId : crypto.randomUUID();
      attempt.current = { body, requestId };
      let failure: unknown;
      try {
        const result = await save({
          id: manifestationId,
          patch: body,
          baseline: metadata.etag,
          requestId,
        });
        if (result.__typename === "MetadataSaved") {
          finish();
          return { values, error: null };
        }
        failure = new MetadataRefusal(result);
      } catch (error) {
        failure = error;
      }
      const recovery = editingFailureAction(failure);
      if ((recovery === "retry" || recovery === "reload") && (await resolvedSaved(requestId))) {
        finish();
        return { values, error: null };
      }
      if (failure instanceof ApiError) {
        const errors = failure.errors;
        return {
          values,
          error: Object.keys(errors ?? {}).length > 0 ? null : formatError(failure),
          errors,
          recovery,
        };
      }
      return {
        values,
        errors: failure instanceof MetadataRefusal ? failure.fieldErrors : undefined,
        error: formatError(failure),
        recovery,
      };
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
