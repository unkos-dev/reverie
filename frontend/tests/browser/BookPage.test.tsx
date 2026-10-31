import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { revertField, type BookDetail } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { BookPage } from "@/pages/book/BookPage";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, acceptVersion: vi.fn(), getBook: vi.fn(), revertField: vi.fn() };
});

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

afterEach(() => {
  vi.resetAllMocks();
});

function bookFixture(overrides: Partial<BookDetail> = {}): BookDetail {
  return {
    id: "abc-123",
    work_id: "w-1",
    title: "The Brothers Karamazov",
    authors: ["Fyodor Dostoevsky"],
    contributors: [],
    series: null,
    description: "A passionate, polyphonic novel of patricide, faith and free will.",
    language: "en",
    isbn_13: "9780374528379",
    isbn_10: "0374528373",
    publisher: null,
    pub_date: null,
    cover_url: "/api/v1/books/abc-123/cover/thumb",
    tags: ["Russian classics"],
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    metadata_version_summary: { pending: 0, accepted: 0 },
    metadata_versions: [],
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

async function renderBook(book: BookDetail): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.books.detail(book.id), book);

  const routes: RouteObject[] = [{ path: "/b/:id", element: <BookPage /> }];
  const router = createMemoryRouter(routes, { initialEntries: [`/b/${book.id}`] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  await render(<Wrapper />);
}

describe("BookPage", () => {
  test("Clear on a draft field asks first and Cancel sends nothing", async () => {
    const user = userEvent;
    await renderBook(
      bookFixture({
        description: "Existing prose",
        metadata_version_summary: { pending: 1, accepted: 0 },
        metadata_versions: [
          {
            id: "v-1",
            field_name: "description",
            source: "openlibrary",
            new_value: "Alt description",
            status: "pending",
            confidence_score: 0.8,
            match_type: "title",
            observation_count: 1,
          },
        ],
      }),
    );
    const versionsTab = page.getByRole("tab", { name: /versions/i });
    await user.click(versionsTab);
    const panel = page.getByRole("tabpanel", { name: /versions/i });
    const clearButton = panel.getByRole("button", { name: /^clear$/i });
    await user.click(clearButton);
    // AlertDialog is portal-rendered outside the panel — search at screen scope.
    const confirmation = page.getByRole("alertdialog", { name: /clear description/i });
    await expect.element(confirmation).toBeInTheDocument();
    expect(revertField).not.toHaveBeenCalled();
    await user.click(confirmation.getByRole("button", { name: "Cancel" }));
    await expect.element(confirmation).not.toBeInTheDocument();
    expect(revertField).not.toHaveBeenCalled();
  });
});
