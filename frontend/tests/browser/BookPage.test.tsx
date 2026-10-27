import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";
import { toast } from "sonner";

import { acceptVersion, getBook, ApiError, type BookDetail } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { BookPage } from "@/pages/book/BookPage";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, acceptVersion: vi.fn(), getBook: vi.fn() };
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

async function renderBook(book: BookDetail, path: string = `/b/${book.id}`): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.books.detail(book.id), book);

  const routes: RouteObject[] = [{ path: "/b/:id", element: <BookPage /> }];
  const router = createMemoryRouter(routes, { initialEntries: [path] });

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
  test("renders the title and author from the cache", async () => {
    await renderBook(bookFixture());
    await expect
      .element(page.getByRole("heading", { name: "The Brothers Karamazov" }))
      .toBeInTheDocument();
    await expect.element(page.getByText("Fyodor Dostoevsky")).toBeInTheDocument();
  });

  test("renders the series label as a link to the series detail", async () => {
    await renderBook(
      bookFixture({
        series: { id: "s-1", name: "Karamazov Trilogy", position: 1 },
      }),
    );
    const label = page.getByRole("link", { name: /Karamazov Trilogy · #1/ });
    await expect.element(label).toHaveAttribute("href", "/series/s-1");
  });

  test("renders the three tabs (Overview, Versions, Activity)", async () => {
    await renderBook(bookFixture());
    await expect.element(page.getByRole("tab", { name: /overview/i })).toBeInTheDocument();
    await expect.element(page.getByRole("tab", { name: /versions/i })).toBeInTheDocument();
    await expect.element(page.getByRole("tab", { name: /activity/i })).toBeInTheDocument();
  });

  test("renders the description in the Overview tab", async () => {
    await renderBook(bookFixture());
    const overviewPanel = page.getByRole("tabpanel", { name: /overview/i });
    await expect
      .element(overviewPanel.getByText(/passionate, polyphonic novel/))
      .toBeInTheDocument();
  });

  test("tabs activate automatically from the keyboard and mount only the selected panel", async () => {
    const user = userEvent;
    await renderBook(bookFixture());
    const tabs = page.getByRole("tablist", { name: "Book details" });
    const overview = tabs.getByRole("tab", { name: "Overview" });
    await expect.element(overview).toHaveAttribute("aria-selected", "true");
    await expect
      .element(page.getByRole("button", { name: /edit metadata/i }))
      .not.toBeInTheDocument();
    expect(page.getByRole("tabpanel").elements()).toHaveLength(1);
    await user.click(overview);
    await user.keyboard("{ArrowRight}");
    await expect.element(tabs.getByRole("tab", { name: "Versions" })).toHaveFocus();
    await expect.element(page.getByRole("tabpanel", { name: "Versions" })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /edit metadata/i })).toBeInTheDocument();
    await user.keyboard("{ArrowRight}");
    await expect.element(page.getByRole("tabpanel", { name: "Activity" })).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /edit metadata/i }))
      .not.toBeInTheDocument();
    await user.keyboard("{Home}");
    await expect.element(overview).toHaveFocus();
    await expect.element(page.getByRole("tabpanel", { name: "Overview" })).toBeInTheDocument();
    expect(page.getByRole("tabpanel").elements()).toHaveLength(1);
    expect(getBook).not.toHaveBeenCalled();
  });

  test("Versions tab badges the pending count and exposes the editor", async () => {
    const user = userEvent;
    await renderBook(
      bookFixture({
        metadata_version_summary: { pending: 3, accepted: 5 },
        metadata_versions: [
          {
            id: "v-1",
            field_name: "title",
            source: "openlibrary",
            new_value: "Alt Title",
            status: "pending",
            confidence_score: 0.91,
            match_type: "isbn",
            observation_count: 1,
          },
          {
            id: "v-2",
            field_name: "title",
            source: "googlebooks",
            new_value: "Another Title",
            status: "pending",
            confidence_score: 0.84,
            match_type: "isbn",
            observation_count: 1,
          },
          {
            id: "v-3",
            field_name: "publisher",
            source: "openlibrary",
            new_value: "Publisher Draft",
            status: "pending",
            confidence_score: 0.8,
            match_type: "title",
            observation_count: 1,
          },
        ],
      }),
    );
    // Tab label badge surfaces the pending count.
    const versionsTab = page.getByRole("tab", { name: /versions/i });
    await expect.element(versionsTab.getByText("3")).toBeInTheDocument();

    await user.click(versionsTab);
    const panel = page.getByRole("tabpanel", { name: /versions/i });
    await expect
      .element(panel.getByRole("heading", { name: /title/i, level: 3 }))
      .toBeInTheDocument();
    expect(panel.getByRole("list").elements()).toHaveLength(2);
    expect(panel.getByRole("listitem").elements()).toHaveLength(5);
    await expect.element(panel.getByText("The Brothers Karamazov")).toBeInTheDocument();
    await expect.element(panel.getByText("unset")).toBeInTheDocument();
    // Pending row exposes the draft value + Accept/Reject buttons.
    await expect.element(panel.getByText("Alt Title")).toBeInTheDocument();
    expect(panel.getByRole("button", { name: /accept/i }).elements()).toHaveLength(3);
    expect(panel.getByRole("button", { name: /reject/i }).elements()).toHaveLength(3);
    // Manual edit button is always present.
    await expect.element(panel.getByRole("button", { name: /edit metadata/i })).toBeInTheDocument();
  });

  test.each(["success", "rejected", "unavailable"])(
    "refreshes the draft list after %s acceptance",
    async (outcome) => {
      const user = userEvent;
      const book = bookFixture({
        metadata_version_summary: { pending: 1, accepted: 0 },
        metadata_versions: [
          {
            id: "v-1",
            field_name: "title",
            source: "openlibrary",
            new_value: "Alt Title",
            status: "pending",
            confidence_score: 0.91,
            match_type: "isbn",
            observation_count: 1,
          },
        ],
      });
      vi.mocked(getBook).mockResolvedValue(bookFixture());
      if (outcome === "success") {
        vi.mocked(acceptVersion).mockResolvedValue(undefined);
      } else {
        vi.mocked(acceptVersion).mockRejectedValue(
          outcome === "rejected"
            ? new ApiError(
                404,
                "https://reverie.example/probs/not-found",
                "Not Found",
                "Resource not found.",
              )
            : new ApiError(503, null, "Service Unavailable", "Try again later."),
        );
      }
      await renderBook(book);
      await user.click(page.getByRole("tab", { name: /versions/i }));
      await expect.element(page.getByText("Alt Title")).toBeInTheDocument();
      expect(getBook).not.toHaveBeenCalled();
      await user.click(page.getByRole("button", { name: /accept/i }));
      await vi.waitFor(async () => {
        await expect.element(page.getByText("Alt Title")).not.toBeInTheDocument();
      });
      await expect.element(page.getByRole("button", { name: /accept/i })).not.toBeInTheDocument();
      expect(acceptVersion).toHaveBeenCalledExactlyOnceWith(book.id, "v-1");
      expect(getBook).toHaveBeenCalledTimes(1);
      if (outcome === "rejected") {
        expect(toast.error).toHaveBeenCalledExactlyOnceWith(
          "This draft is no longer available to accept.",
        );
      } else if (outcome === "unavailable") {
        expect(toast.error).toHaveBeenCalledExactlyOnceWith(
          "Service Unavailable: Try again later.",
        );
      }
    },
  );

  test("falls back to italic placeholder when description is null", async () => {
    await renderBook(bookFixture({ description: null }));
    await expect.element(page.getByText(/no description yet/i)).toBeInTheDocument();
  });

  test("Clear button on a draft field opens the confirmation AlertDialog", async () => {
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
    await expect
      .element(page.getByRole("alertdialog", { name: /clear description/i }))
      .toBeInTheDocument();
  });

  test("cover load error swaps in the cloth spine", async () => {
    await renderBook(bookFixture());
    const img = page.getByRole("img", { name: /Cover of/ });
    expect(document.querySelector("[data-cloth]")).toBeNull();
    await expect.element(img).not.toBeInTheDocument();
    await vi.waitFor(() => {
      expect(document.querySelector("[data-cloth]")).not.toBeNull();
    });
  });

  test("back link points to /library", async () => {
    await renderBook(bookFixture());
    const back = page.getByRole("link", { name: /library/i });
    expect(back.element().getAttribute("href")).toBe("/library");
  });
});
