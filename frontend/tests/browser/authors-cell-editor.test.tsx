import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, expect, test, vi } from "vite-plus/test";
import { RelayEnvironmentProvider } from "react-relay";
import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import type { BookListItem } from "@/api";
import { AuthorsCell } from "@/pages/library/table/cells";

const book: BookListItem = {
  metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
  reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
  id: encodeGraphQLId("Manifestation", "11111111-1111-4111-8111-111111111111"),
  work_id: encodeGraphQLId("Work", "22222222-2222-4222-8222-222222222222"),
  title: "A Shared Work",
  subtitle: null,
  authors: ["Ann Able", "Ben Baker", "Cal Cole"],
  contributors: [],
  series: null,
  isbn_13: null,
  pages: null,
  tags: [],
  genres: [],
  moods: [],
  content_rating: null,
  cover_url: "/api/v1/books/11111111-1111-1111-1111-111111111111/cover",
  ingestion_status: "complete",
  validation_status: "clean",
  enrichment_status: "complete",
  reading_state: null,
  created_at: "2026-01-01T00:00:00Z",
};

afterEach(() => {
  vi.restoreAllMocks();
  setRelayAccount(null);
});

test("saving stores the displayed authors in displayed order, including a typed name", async () => {
  // REV-REQ-0019: the ordered replacement retains its selected values and matching revision.
  __seedCsrfTokenForTesting("synthetic-author-editor-csrf-value");
  setRelayAccount("author-editor");
  vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(
      JSON.stringify({
        data: {
          manifestation: {
            id: book.id,
            metadata: {
              id: `metadata_rev_${book.id}_${book.metadata_etag.replaceAll('"', "")}`,
              manifestationId: book.id,
              workId: book.work_id,
              etag: book.metadata_etag,
              authors: {
                edges: book.authors.map((name, index) => ({
                  cursor: String(index),
                  node: {
                    name,
                    author: {
                      id: encodeGraphQLId(
                        "Author",
                        `30000000-0000-4000-8000-${String(index).padStart(12, "0")}`,
                      ),
                    },
                  },
                })),
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                  startCursor: "0",
                  endCursor: "2",
                },
              },
            },
          },
        },
      }),
      { headers: { "Content-Type": "application/json" } },
    ),
  );
  const onCommit = vi.fn();
  await render(
    <RelayEnvironmentProvider environment={relayEnvironment()}>
      <AuthorsCell
        book={book}
        density="comfortable"
        state={{ pending: undefined, failed: false }}
        onCommit={onCommit}
      />
    </RelayEnvironmentProvider>,
  );
  await userEvent.click(page.getByRole("button", { name: /edit authors/ }));
  const field = page.getByRole("textbox", { name: "Add an author" });
  await expect.element(field).toHaveFocus();

  await userEvent.keyboard("{Shift>}{Tab}{Tab}{/Shift}");
  await expect.element(page.getByRole("button", { name: "Drag Cal Cole" })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByRole("button", { name: "Insert after Cal Cole" })).toHaveFocus();
  await userEvent.keyboard("{ArrowLeft}{ArrowLeft}{ArrowLeft}");
  await expect.element(page.getByRole("button", { name: "Insert before Ann Able" })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  await expect
    .element(page.getByRole("grid", { name: "Authors" }).getByRole("row").first())
    .toHaveAccessibleName("Cal Cole");

  await userEvent.fill(field, "Dee Dunn");
  await userEvent.click(page.getByRole("button", { name: "Save" }));

  expect(onCommit).toHaveBeenCalledTimes(1);
  expect(onCommit).toHaveBeenCalledWith(
    ["Cal Cole", "Ann Able", "Ben Baker", "Dee Dunn"],
    book.metadata_etag,
    book.authors,
  );
});
