import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import { RelayEnvironmentProvider, loadQuery } from "react-relay";
import { z } from "zod";
import trustedDocuments from "../../../backend/graphql-documents.json";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import { readerDetailQuery } from "@/lib/relay/reader";
import type { readerDetailQuery as DetailQuery } from "@/lib/relay/__generated__/readerDetailQuery.graphql";
import { BookPage } from "@/pages/book/BookPage";

const documents: Record<string, string> = trustedDocuments;
afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
  setRelayAccount(null);
});
describe("BookPage", () => {
  // Data loss boundary: dismissing a destructive field confirmation sends no mutation.
  test("Clear on a draft field asks first and Cancel sends nothing", async () => {
    setRelayAccount("book-review");
    const id = encodeGraphQLId("Manifestation", "11111111-1111-4111-8111-111111111111");
    const pageInfo = {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    };
    let clears = 0;
    vi.spyOn(globalThis, "fetch").mockImplementation((_input, init) => {
      const request = z.object({ id: z.string() }).parse(JSON.parse(z.string().parse(init?.body)));
      const name = documents[request.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
      if (name === "mutationsRevertMetadataFieldMutation") clears += 1;
      const manifestation = {
        id,
        coverUrl: "",
        isbn10: "0374528373",
        isbn13: "9780374528379",
        isbnMismatch: false,
        publisher: null,
        publicationDate: null,
        ingestionStatus: "complete",
        validationStatus: "clean",
        enrichmentStatus: "complete",
        createdAt: "2026-01-01T00:00:00Z",
        work: {
          id: encodeGraphQLId("Work", "22222222-2222-4222-8222-222222222222"),
          title: "The Brothers Karamazov",
          description: "Existing prose",
          language: "en",
          selectedSeries: null,
          authors: {
            edges: [
              {
                cursor: "author",
                node: {
                  id: encodeGraphQLId("Author", "33333333-3333-4333-8333-333333333333"),
                  name: "Fyodor Dostoevsky",
                },
              },
            ],
            pageInfo,
          },
        },
        tags: { edges: [], pageInfo },
        versions: {
          pendingCount: 1,
          pending: {
            edges: [
              {
                cursor: "version",
                node: {
                  id: encodeGraphQLId("MetadataVersion", "44444444-4444-4444-8444-444444444444"),
                  fieldName: "description",
                  source: "openlibrary",
                  newValue: "Alt description",
                  status: "pending",
                  confidenceScore: 0.8,
                  matchType: "title",
                  observationCount: 1,
                },
              },
            ],
            pageInfo,
          },
        },
      };
      return Promise.resolve(
        new Response(JSON.stringify({ data: { manifestation } }), {
          headers: { "Content-Type": "application/json" },
        }),
      );
    });
    const environment = relayEnvironment();
    const router = createMemoryRouter(
      [
        {
          path: "/b/:id",
          element: <BookPage />,
          loader: () => {
            const reference = loadQuery<DetailQuery>(
              environment,
              readerDetailQuery,
              { id },
              { fetchPolicy: "network-only" },
            );
            return {
              reference,
              dispose: () => {
                reference.dispose();
              },
            };
          },
        },
      ],
      { initialEntries: [`/b/${id}`] },
    );
    await render(
      <RelayEnvironmentProvider environment={environment}>
        <RouterProvider router={router} />
      </RelayEnvironmentProvider>,
    );
    await userEvent.click(page.getByRole("tab", { name: /versions/i }));
    await userEvent.click(
      page.getByRole("tabpanel", { name: /versions/i }).getByRole("button", { name: /^clear$/i }),
    );
    const confirmation = page.getByRole("alertdialog", { name: /clear description/i });
    await expect.element(confirmation).toBeInTheDocument();
    expect(clears).toBe(0);
    await userEvent.click(confirmation.getByRole("button", { name: "Cancel" }));
    await expect.element(confirmation).not.toBeInTheDocument();
    expect(clears).toBe(0);
  });
});
