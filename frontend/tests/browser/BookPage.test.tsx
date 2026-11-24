import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import { RelayEnvironmentProvider, loadQuery } from "react-relay";
import { createMockEnvironment } from "relay-test-utils";
import bookPageQuery, {
  type BookPageQuery as DetailQuery,
} from "@/lib/relay/__generated__/BookPageQuery.graphql";
import versionsTabQuery from "@/lib/relay/__generated__/VersionsTabQuery.graphql";
import { BookPage } from "@/pages/book/BookPage";

afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
});
describe("BookPage", () => {
  // Data loss boundary: dismissing a destructive field confirmation sends no mutation.
  test("Clear on a draft field asks first and Cancel sends nothing", async () => {
    const id = "manifestation";
    const pageInfo = {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    };
    const manifestation = {
      id,
      __typename: "Manifestation",
      coverUrl: "",
      isbn10: "0374528373",
      isbn13: "9780374528379",
      isbnMismatch: false,
      publisher: null,
      publicationDate: null,
      ingestionStatus: "COMPLETE",
      validationStatus: "CLEAN",
      enrichmentStatus: "COMPLETE",
      createdAt: "2026-01-01T00:00:00Z",
      work: {
        id: "work",
        __typename: "Work",
        title: "The Brothers Karamazov",
        description: "Existing prose",
        language: "en",
        selectedSeries: null,
        detailAuthors: {
          edges: [
            {
              cursor: "author",
              node: {
                id: "author",
                __typename: "Author",
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
                id: "pending",
                __typename: "MetadataVersion",
                fieldName: "description",
                source: "openlibrary",
                newValue: { __typename: "MetadataProposalText", text: "Alt description" },
                status: "PENDING",
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
    const environment = createMockEnvironment();
    const writes = vi.spyOn(environment, "executeMutation");
    function resolveRead(query: typeof bookPageQuery): Promise<void> {
      return vi.waitFor(() => {
        environment.mock.resolve(query, { data: { manifestation } });
      });
    }
    const router = createMemoryRouter(
      [
        {
          path: "/b/:id",
          element: <BookPage />,
          loader: () => {
            const reference = loadQuery<DetailQuery>(
              environment,
              bookPageQuery,
              { id },
              { fetchPolicy: "network-only" },
            );
            return reference;
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
    await resolveRead(bookPageQuery);
    await userEvent.click(page.getByRole("tab", { name: /versions/i }));
    await resolveRead(versionsTabQuery);
    await userEvent.click(
      page.getByRole("tabpanel", { name: /versions/i }).getByRole("button", { name: /^clear$/i }),
    );
    const confirmation = page.getByRole("alertdialog", { name: /clear description/i });
    await expect.element(confirmation).toBeInTheDocument();
    expect(writes).not.toHaveBeenCalled();
    await userEvent.click(confirmation.getByRole("button", { name: "Cancel" }));
    await expect.element(confirmation).not.toBeInTheDocument();
    expect(writes).not.toHaveBeenCalled();
  });
});
