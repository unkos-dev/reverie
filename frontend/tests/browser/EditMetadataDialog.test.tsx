import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, expect, test, vi } from "vite-plus/test";
import { RelayEnvironmentProvider } from "react-relay";
import { ViewTransition } from "react";
import { z } from "zod";
import trustedDocuments from "../../../backend/graphql-documents.json";
import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import { EditMetadataDialog, type EditableField } from "@/pages/book/EditMetadataDialog";

const ID = "11111111-1111-4111-8111-111111111111";
const WORK = "22222222-2222-4222-8222-222222222222";
const FIELDS: readonly EditableField[] = [{ name: "subtitle", label: "Subtitle" }];
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
function failure(code: string, errors?: Record<string, string>): Response {
  return new Response(
    JSON.stringify({
      errors: [{ message: "Invalid publication date.", extensions: { code, errors } }],
    }),
    { headers: { "Content-Type": "application/json" } },
  );
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
      return Promise.resolve(handlers.save(saveSchema.parse(request.variables)));
    if (name === "mutationsResolveFormQuery" && handlers.resolve !== undefined)
      return Promise.resolve(handlers.resolve());
    throw new Error(`Unexpected operation ${name ?? request.id}`);
  });
}
async function renderDialog(fields: readonly EditableField[] = FIELDS) {
  const onOpenChange = vi.fn();
  const result = await render(
    <RelayEnvironmentProvider environment={relayEnvironment()}>
      <ViewTransition>
        <EditMetadataDialog manifestationId={ID} open onOpenChange={onOpenChange} fields={fields} />
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

// REV-REQ-0019: a form opens from a fresh matched snapshot, including when the previous read is cached.
test("seeds from a fresh metadata snapshot and reopens with saved values", async () => {
  let current = snapshot("Fetched Subtitle");
  let reads = 0;
  let finishConfirmation: ((value: Response) => void) | undefined;
  const confirmation = new Promise<Response>((resolve) => {
    finishConfirmation = resolve;
  });
  intercept({
    read: () => {
      reads += 1;
      if (reads === 2) return confirmation;
      return response({ manifestation: { id: current.manifestationId, metadata: current } });
    },
    save: ({ patch }) => {
      current = {
        ...current,
        id: revisionId('"bbbbbbbbbbbbbbbbbbbbbb"'),
        subtitle: patch.subtitle ?? "",
        etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
      };
      return response({ updateMetadata: { metadata: current, operations: [], requestId: null } });
    },
  });
  const first = await renderDialog();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Fetched Subtitle");
  await page.getByLabelText("Subtitle").fill("Updated Subtitle");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  await vi.waitFor(() => {
    expect(reads).toBe(2);
  });
  expect(first.onOpenChange).not.toHaveBeenCalled();
  await expect.element(page.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
  finishConfirmation?.(
    response({ manifestation: { id: current.manifestationId, metadata: current } }),
  );
  await vi.waitFor(() => {
    expect(first.onOpenChange).toHaveBeenCalledWith(false);
  });
  await first.unmount();
  await renderDialog();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  expect(reads).toBe(3);
});

// Data loss boundary: failed and uncertain writes retain values, the opening baseline and unchanged-attempt identity.
test("awaits save, retains publication-date errors and other edits, and closes after correction", async () => {
  const opening = snapshot();
  const writes: z.infer<typeof saveSchema>[] = [];
  let finish: ((value: Response) => void) | undefined;
  let saves = 0;
  let resolutions = 0;
  intercept({
    read: () => response({ manifestation: { id: opening.manifestationId, metadata: opening } }),
    save: (variables) => {
      writes.push(variables);
      saves += 1;
      if (saves === 1)
        return new Promise<Response>((resolve) => {
          finish = resolve;
        });
      if (saves === 2) return Promise.reject(new TypeError("Connection lost"));
      return failure("if-match-mismatch");
    },
    resolve: () => {
      resolutions += 1;
      return response({
        metadataOperation: {
          metadata: snapshot("Updated Subtitle", '"bbbbbbbbbbbbbbbbbbbbbb"'),
          outcome: resolutions === 1 ? "unknown" : "saved",
          operations: [],
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
  finish?.(failure("validation", { pub_date: "Use YYYY-MM-DD or YYYY. Leave empty if unknown." }));
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
    read: () => response({ manifestation: { id: opening.manifestationId, metadata: opening } }),
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
  acknowledge?.(
    response({
      updateMetadata: { metadata: snapshot("Dismissed edit"), requestId: null, operations: [] },
    }),
  );
  expect(dismissed.onOpenChange).not.toHaveBeenCalled();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Draft from reopened dialog");
});

// Data loss boundary: clearing a populated value requires explicit confirmation.
test("cancelling a populated-field clear keeps the edit and sends nothing; confirmation saves null", async () => {
  const writes: z.infer<typeof saveSchema>[] = [];
  const current = snapshot();
  intercept({
    read: () => response({ manifestation: { id: current.manifestationId, metadata: current } }),
    save: (variables) => {
      writes.push(variables);
      return response({
        updateMetadata: {
          metadata: { ...current, subtitle: null },
          requestId: variables.requestId,
          operations: [],
        },
      });
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
