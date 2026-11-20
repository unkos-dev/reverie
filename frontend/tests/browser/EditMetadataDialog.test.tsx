import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, expect, test, vi } from "vite-plus/test";
import { RelayEnvironmentProvider, loadQuery } from "react-relay";
import { ViewTransition } from "react";
import { z } from "zod";
import trustedDocuments from "../../../backend/graphql-documents.json";
import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import { mutationsFormQuery } from "@/lib/relay/mutations";
import type { mutationsFormQuery as FormQuery } from "@/lib/relay/__generated__/mutationsFormQuery.graphql";
import { EditMetadataDialog, type EditableField } from "@/pages/book/EditMetadataDialog";

const ID = "11111111-1111-4111-8111-111111111111";
const WORK = "22222222-2222-4222-8222-222222222222";
const FIELDS: readonly EditableField[] = [{ name: "subtitle", label: "Subtitle" }];
const GRAPH_ID = encodeGraphQLId("Manifestation", ID);
const documents: Record<string, string> = trustedDocuments;
const requestSchema = z.object({ id: z.string(), variables: z.record(z.string(), z.unknown()) });
const saveSchema = z.object({
  id: z.string(),
  baseline: z.string(),
  requestId: z.uuid(),
  patch: z.object({
    subtitle: z.string().nullable().optional(),
    publicationDate: z.string().nullable().optional(),
  }),
});
const revisionIds = new Map<string, string>();
function revisionId(etag: string): string {
  const known = revisionIds.get(etag);
  if (known !== undefined) return known;
  const created = encodeGraphQLId("MetadataRevision", crypto.randomUUID());
  revisionIds.set(etag, created);
  return created;
}
function snapshot(subtitle = "Original Subtitle", etag = '"aaaaaaaaaaaaaaaaaaaaaa"') {
  return {
    id: revisionId(etag),
    workId: encodeGraphQLId("Work", WORK),
    manifestationId: encodeGraphQLId("Manifestation", ID),
    etag,
    title: "Original Title",
    subtitle,
    description: null,
    language: null,
    publisher: null,
    publicationDate: null,
    isbn10: null,
    isbn13: null,
    isbnMismatch: false,
    pages: null,
    contentRating: null,
  };
}
function response(data: unknown): Response {
  return new Response(JSON.stringify({ data }), {
    headers: { "Content-Type": "application/json" },
  });
}
function saved(
  metadata: Omit<ReturnType<typeof snapshot>, "subtitle"> & { subtitle: string | null },
): Response {
  return response({
    updateMetadata: {
      __typename: "MetadataSaved",
      manifestation: {
        id: GRAPH_ID,
        metadata,
        isbn13: metadata.isbn13,
        isbn10: metadata.isbn10,
        isbnMismatch: metadata.isbnMismatch,
        publisher: metadata.publisher,
        publicationDate: metadata.publicationDate,
        work: {
          id: metadata.workId,
          title: metadata.title,
          description: metadata.description,
          language: metadata.language,
        },
      },
    },
  });
}
function refused(refusal: Record<string, unknown>): Response {
  return response({ updateMetadata: refusal });
}
function intercept(handlers: {
  read: () => Response | Promise<Response>;
  save?: (variables: z.infer<typeof saveSchema>) => Response | Promise<Response>;
  resolve?: () => Response | Promise<Response>;
}): void {
  vi.spyOn(globalThis, "fetch").mockImplementation((_input, init) => {
    const request = requestSchema.parse(JSON.parse(z.string().parse(init?.body)));
    const name = documents[request.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
    if (name === "mutationsFormQuery") return Promise.resolve(handlers.read());
    if (name === "mutationsSaveFormMutation" && handlers.save !== undefined)
      return Promise.resolve(handlers.save(saveSchema.parse(request.variables.input)));
    if (name === "mutationsResolveFormQuery" && handlers.resolve !== undefined)
      return Promise.resolve(handlers.resolve());
    throw new Error(`Unexpected operation ${name ?? request.id}`);
  });
}
async function renderDialog(fields: readonly EditableField[] = FIELDS) {
  const onOpenChange = vi.fn();
  const environment = relayEnvironment();
  const queryRef = loadQuery<FormQuery>(environment, mutationsFormQuery, { id: GRAPH_ID });
  const result = await render(
    <RelayEnvironmentProvider environment={environment}>
      <ViewTransition>
        <EditMetadataDialog
          queryRef={queryRef}
          load={vi.fn()}
          open
          onOpenChange={onOpenChange}
          fields={fields}
        />
      </ViewTransition>
    </RelayEnvironmentProvider>,
  );
  return { ...result, onOpenChange };
}
beforeEach(() => {
  __seedCsrfTokenForTesting("synthetic-metadata-form-csrf-token");
  setRelayAccount("metadata-form");
});
afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
  setRelayAccount(null);
});

// Data loss boundary: failed and uncertain writes retain values, the opening baseline and unchanged-attempt identity.
test("awaits save, retains publication-date errors and other edits, and closes after correction", async () => {
  const opening = snapshot();
  const writes: z.infer<typeof saveSchema>[] = [];
  let finish: ((value: Response) => void) | undefined;
  let saves = 0;
  let resolutions = 0;
  intercept({
    read: () => response({ manifestation: { id: GRAPH_ID, metadata: opening } }),
    save: (variables) => {
      writes.push(variables);
      saves += 1;
      if (saves === 1)
        return new Promise<Response>((resolve) => {
          finish = resolve;
        });
      if (saves === 2) return Promise.reject(new TypeError("Connection lost"));
      return refused({ __typename: "StaleBaseline", message: "The baseline is stale." });
    },
    resolve: () => {
      resolutions += 1;
      return response({
        metadataOperation: {
          metadata: snapshot("Updated Subtitle", '"bbbbbbbbbbbbbbbbbbbbbb"'),
          outcome: resolutions === 1 ? "unknown" : "saved",
        },
      });
    },
  });
  const rendered = await renderDialog([...FIELDS, { name: "pub_date", label: "Publication date" }]);
  await page.getByLabelText("Publication date").fill("unknown");
  await page.getByLabelText("Subtitle").fill("Updated Subtitle");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  await expect
    .element(page.getByRole("button", { name: "Save", exact: true }))
    .toHaveAttribute("data-pending");
  await expect.element(page.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  finish?.(
    refused({
      __typename: "MetadataInvalid",
      message: "Invalid publication date.",
      fieldErrors: [
        { field: "pub_date", message: "Use YYYY-MM-DD or YYYY. Leave empty if unknown." },
      ],
    }),
  );
  await expect
    .element(page.getByLabelText("Publication date"))
    .toHaveAttribute("aria-invalid", "true");
  await expect.element(page.getByLabelText("Publication date")).toHaveValue("unknown");
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  await page.getByLabelText("Publication date").fill("2024-06");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  await expect
    .element(page.getByRole("button", { name: "Retry", exact: true }))
    .toBeInTheDocument();
  expect(writes[1]?.baseline).toBe(opening.etag);
  expect(writes[1]?.requestId).not.toBe(writes[0]?.requestId);
  await userEvent.click(page.getByRole("button", { name: "Retry", exact: true }));
  await vi.waitFor(() => {
    expect(rendered.onOpenChange).toHaveBeenCalledWith(false);
  });
  expect(writes[2]).toEqual(writes[1]);
  expect(writes).toHaveLength(3);
  expect(resolutions).toBe(2);
  await rendered.unmount();
  let acknowledge: ((value: Response) => void) | undefined;
  vi.restoreAllMocks();
  intercept({
    read: () => response({ manifestation: { id: GRAPH_ID, metadata: opening } }),
    save: () =>
      new Promise<Response>((resolve) => {
        acknowledge = resolve;
      }),
  });
  const dismissed = await renderDialog();
  await page.getByLabelText("Subtitle").fill("Dismissed edit");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  await expect
    .element(page.getByRole("button", { name: "Save", exact: true }))
    .toHaveAttribute("data-pending");
  await dismissed.unmount();
  await renderDialog();
  await page.getByLabelText("Subtitle").fill("Draft from reopened dialog");
  acknowledge?.(saved(snapshot("Dismissed edit")));
  expect(dismissed.onOpenChange).not.toHaveBeenCalled();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Draft from reopened dialog");
});

// Data loss boundary: clearing a populated value requires explicit confirmation.
test("cancelling a populated-field clear keeps the edit and sends nothing; confirmation saves null", async () => {
  const writes: z.infer<typeof saveSchema>[] = [];
  const current = snapshot();
  intercept({
    read: () => response({ manifestation: { id: GRAPH_ID, metadata: current } }),
    save: (variables) => {
      writes.push(variables);
      return saved({ ...current, subtitle: null });
    },
  });
  const rendered = await renderDialog();
  await page.getByLabelText("Subtitle").fill("");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  const confirmation = page.getByRole("alertdialog", { name: "Clear populated field?" });
  await expect.element(confirmation).toBeInTheDocument();
  await userEvent.click(confirmation.getByRole("button", { name: "Cancel", exact: true }));
  expect(writes).toHaveLength(0);
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  await userEvent.click(confirmation.getByRole("button", { name: "Clear and save" }));
  await vi.waitFor(() => {
    expect(rendered.onOpenChange).toHaveBeenCalledWith(false);
  });
  expect(writes.at(0)?.patch).toEqual({ subtitle: null });
});
