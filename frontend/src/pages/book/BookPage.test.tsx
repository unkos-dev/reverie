import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import type { BookDetail } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { BookPage } from "./BookPage";

function bookFixture(overrides: Partial<BookDetail> = {}): BookDetail {
  return {
    id: "abc-123",
    work_id: "w-1",
    title: "The Brothers Karamazov",
    authors: ["Fyodor Dostoevsky"],
    series: null,
    description: "A passionate, polyphonic novel of patricide, faith and free will.",
    language: "en",
    isbn_13: "9780374528379",
    isbn_10: "0374528373",
    cover_url: "/api/books/abc-123/cover/thumb",
    tags: ["Russian classics"],
    ingestion_status: "complete",
    validation_status: "valid",
    enrichment_status: "complete",
    metadata_version_summary: { pending: 0, accepted: 0 },
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function renderBook(book: BookDetail, path: string = `/b/${book.id}`): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
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

  render(<Wrapper />);
}

describe("BookPage", () => {
  test("renders the title and author from the cache", async () => {
    renderBook(bookFixture());
    expect(
      await screen.findByRole("heading", { name: "The Brothers Karamazov" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Fyodor Dostoevsky")).toBeInTheDocument();
  });

  test("renders the series label when present", async () => {
    renderBook(
      bookFixture({
        series: { id: "s-1", name: "Karamazov Trilogy", position: 1 },
      }),
    );
    expect(await screen.findByText(/Karamazov Trilogy · #1/)).toBeInTheDocument();
  });

  test("renders the three tabs (Overview, Versions, Activity)", async () => {
    renderBook(bookFixture());
    expect(await screen.findByRole("tab", { name: /overview/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /versions/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /activity/i })).toBeInTheDocument();
  });

  test("renders the description in the Overview tab", async () => {
    renderBook(bookFixture());
    const overviewPanel = await screen.findByRole("tabpanel", { name: /overview/i });
    expect(within(overviewPanel).getByText(/passionate, polyphonic novel/)).toBeInTheDocument();
  });

  test("Versions tab shows pending + accepted counts", async () => {
    const user = userEvent.setup();
    renderBook(
      bookFixture({
        metadata_version_summary: { pending: 3, accepted: 5 },
      }),
    );
    // Tab label badge surfaces the pending count.
    const versionsTab = await screen.findByRole("tab", { name: /versions/i });
    expect(within(versionsTab).getByText("3")).toBeInTheDocument();

    // Switch to the Versions panel and assert the accepted count
    // (rendered inside the panel body next to "Accepted versions").
    await user.click(versionsTab);
    const panel = await screen.findByRole("tabpanel", { name: /versions/i });
    expect(within(panel).getByText("Pending drafts")).toBeInTheDocument();
    expect(within(panel).getByText("Accepted versions")).toBeInTheDocument();
    expect(within(panel).getByText("5")).toBeInTheDocument();
  });

  test("falls back to italic placeholder when description is null", async () => {
    renderBook(bookFixture({ description: null }));
    expect(await screen.findByText(/no description yet/i)).toBeInTheDocument();
  });

  test("back link points to /library", async () => {
    renderBook(bookFixture());
    const back = await screen.findByRole("link", { name: /library/i });
    expect(back.getAttribute("href")).toBe("/library");
  });
});
