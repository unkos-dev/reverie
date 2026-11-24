/** Manual metadata editing keeps the revision it opened with as its baseline through save recovery. */
import {
  Suspense,
  useActionState,
  useEffect,
  useId,
  useLayoutEffect,
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
  graphql,
  useFragment,
  useMutation,
  usePreloadedQuery,
  useRelayEnvironment,
  type PreloadedQuery,
} from "react-relay";
import { type FetchPolicy } from "relay-runtime";
import { Popover, PopoverContent } from "@/components/ui/popover";
import { FieldError } from "@/components/ui/field";

import { ApiError, UpdateBookMetadataFieldsSchema, type UpdateBookMetadataFields } from "@/api";
import { editingFailureAction } from "@/api/graphql";
import { invalidateLibraryVisits, siblingEditionsForFields } from "@/lib/relay/library-visit";
import type { EditMetadataDialogQuery as FormQuery } from "@/lib/relay/__generated__/EditMetadataDialogQuery.graphql";
import type {
  EditMetadataDialog_opening$data as FormSnapshot,
  EditMetadataDialog_opening$key,
} from "@/lib/relay/__generated__/EditMetadataDialog_opening.graphql";
import type {
  EditMetadataDialogSaveMutation as SaveForm,
  MetadataPatch,
  UpdateMetadataInput,
} from "@/lib/relay/__generated__/EditMetadataDialogSaveMutation.graphql";
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

const formQuery = graphql`
  query EditMetadataDialogQuery($id: ID!) @throwOnFieldError {
    manifestation(id: $id) {
      id
      metadata {
        ...EditMetadataDialog_opening
      }
    }
  }
`;

const openingFragment = graphql`
  fragment EditMetadataDialog_opening on MetadataRevision @throwOnFieldError {
    etag
    title
    subtitle
    description
    language
    publisher
    publicationDate
    isbn10
    isbn13
    isbnMismatch
  }
`;

const saveFormMutation = graphql`
  mutation EditMetadataDialogSaveMutation($input: UpdateMetadataInput!) {
    updateMetadata(input: $input) {
      __typename
      ... on MetadataSaved {
        manifestation {
          id
          metadata {
            ...EditMetadataDialog_opening
          }
          work {
            id
          }
          ...LibraryTable_cell
          ...BookPage_book
          ...VersionsTab_proposals
        }
      }
      ... on StaleBaseline {
        message
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          field
          message
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
  const [opening, setOpening] = useState<(OpeningValues & { generation: number }) | null>(null);
  const [captureReload, setCaptureReload] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [reloadError, setReloadError] = useState(false);
  const reloadRequest = useRef<{ unsubscribe: () => void } | null>(null);
  useEffect(
    () => () => {
      reloadRequest.current?.unsubscribe();
    },
    [],
  );

  const read = (
    <MetadataOpening
      queryRef={queryRef}
      onRead={(values) => {
        setOpening((previous) => ({ ...values, generation: (previous?.generation ?? 0) + 1 }));
        setCaptureReload(false);
        setReloading(false);
        setReloadError(false);
      }}
    />
  );
  if (opening === null) return read;
  const reload = (): void => {
    if (reloading) return;
    setReloading(true);
    let loaded = false;
    reloadRequest.current = fetchQuery<FormQuery>(
      environment,
      formQuery,
      { id: opening.manifestationId },
      { fetchPolicy: "network-only" },
    ).subscribe({
      next: (result) => {
        loaded = result.manifestation != null;
      },
      complete: () => {
        if (loaded) setCaptureReload(true);
        else {
          setReloading(false);
          setReloadError(true);
        }
      },
      error: () => {
        setReloading(false);
        setReloadError(true);
      },
    });
  };
  return (
    <>
      {captureReload && (
        <ErrorBoundary
          fallback={null}
          onError={() => {
            setCaptureReload(false);
            setReloading(false);
            setReloadError(true);
          }}
        >
          <Suspense fallback={null}>{read}</Suspense>
        </ErrorBoundary>
      )}
      <EditMetadataFields
        key={opening.generation}
        onReload={reload}
        reloading={reloading}
        reloadError={reloadError}
        manifestationId={opening.manifestationId}
        fields={fields}
        metadata={opening.metadata}
        onDone={onDone}
      />
    </>
  );
}

type OpeningValues = { manifestationId: string; metadata: FormSnapshot };

function MetadataOpening({
  queryRef,
  onRead,
}: Readonly<{
  queryRef: PreloadedQuery<FormQuery>;
  onRead: (values: OpeningValues) => void;
}>): null {
  const { manifestation } = usePreloadedQuery<FormQuery>(formQuery, queryRef);
  const metadata = useFragment<EditMetadataDialog_opening$key>(
    openingFragment,
    manifestation?.metadata,
  );
  useLayoutEffect(() => {
    if (manifestation != null && metadata != null)
      onRead({ manifestationId: manifestation.id, metadata });
  }, [manifestation, metadata, onRead]);
  if (manifestation == null || metadata == null) throw new Error("Book not found.");
  return null;
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
  metadata,
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
  const [commitSave] = useMutation<SaveForm>(saveFormMutation);
  const attempt = useRef<MetadataPatch | null>(null);
  const touched = useRef(new Set<keyof UpdateBookMetadataFields>());
  const [pendingClearConfirm, setPendingClearConfirm] = useState(false);
  const formId = useId();
  function buildBody(values: Record<string, string>): { patch: MetadataPatch; fields: string[] } {
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
    const patch: MetadataPatch = {
      ...("title" in body ? { title: body.title } : {}),
      ...("subtitle" in body ? { subtitle: body.subtitle } : {}),
      ...("description" in body ? { description: body.description } : {}),
      ...("language" in body ? { language: body.language } : {}),
      ...("publisher" in body ? { publisher: body.publisher } : {}),
      ...("pub_date" in body ? { publicationDate: body.pub_date } : {}),
      ...("isbn_10" in body ? { isbn10: body.isbn_10 } : {}),
      ...("isbn_13" in body ? { isbn13: body.isbn_13 } : {}),
    };
    return { patch, fields: Object.keys(body) };
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
      let change: ReturnType<typeof buildBody>;
      try {
        change = buildBody(values);
      } catch (err) {
        console.error("[EditMetadataDialog.buildBody] schema parse failed", err);
        const error = "Form has an invalid value. Refresh and try again.";
        return { values, error };
      }
      if (change.fields.length === 0) {
        const error = "No fields touched — nothing to save.";
        return { values, error };
      }
      setPendingClearConfirm(false);
      const { patch, fields: changedFields } = change;
      const previous = attempt.current;
      const unchanged = previous !== null && JSON.stringify(previous) === JSON.stringify(patch);
      if (unchanged && _previous.recovery !== undefined && _previous.recovery !== "retry")
        return { ..._previous, values };
      attempt.current = patch;
      let result: SaveForm["response"]["updateMetadata"];
      try {
        result = await save({ id: manifestationId, patch, baseline: metadata.etag });
      } catch (failure) {
        const recovery = editingFailureAction(failure);
        const errors = failure instanceof ApiError ? failure.errors : undefined;
        return {
          values,
          error: Object.keys(errors ?? {}).length > 0 ? null : formatError(failure),
          errors,
          recovery,
        };
      }
      switch (result.__typename) {
        case "MetadataSaved": {
          const { id, work } = result.manifestation;
          invalidateLibraryVisits(environment, changedFields, {
            siblingEditions: siblingEditionsForFields(changedFields, id, work?.id),
          });
          finish();
          return { values, error: null };
        }
        case "StaleBaseline":
          return { values, error: result.message, recovery: "reload" };
        case "MetadataInvalid":
          return {
            values,
            error: result.message,
            errors: Object.fromEntries(
              result.fieldErrors.map(({ field, message }) => [field, message]),
            ),
            recovery: "correct",
          };
        case "FieldsLocked":
        case "NotFound":
          return { values, error: result.message, recovery: "none" };
        default:
          return { values, error: "Request failed.", recovery: "none" };
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
