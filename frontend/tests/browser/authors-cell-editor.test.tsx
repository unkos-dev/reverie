import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { expect, test, vi } from "vite-plus/test";
import type { BookListItem } from "@/api";
import { AuthorsCell } from "@/pages/library/table/cells";

const book: BookListItem = {
  metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
  reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
  id: "11111111-1111-1111-1111-111111111111",
  work_id: "22222222-2222-2222-2222-222222222222",
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

test("saving stores the displayed authors in displayed order, including a typed name", async () => {
  const onCommit = vi.fn();
  await render(
    <AuthorsCell
      book={book}
      density="comfortable"
      state={{ pending: undefined, failed: false }}
      onCommit={onCommit}
    />,
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
  expect(onCommit).toHaveBeenCalledWith(["Cal Cole", "Ann Able", "Ben Baker", "Dee Dunn"]);
});
