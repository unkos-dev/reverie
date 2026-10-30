import { describe, expect, test } from "vite-plus/test";

import type { BookListItem } from "@/api";

import { EDIT_ROUTES } from "./edit-routing";

function rowFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: "book-1",
    work_id: "work-1",
    title: "Original Title",
    subtitle: "Original Subtitle",
    authors: ["Original Author"],
    contributors: [],
    series: null,
    isbn_13: "9780000000001",
    pages: 100,
    tags: [],
    genres: [],
    moods: [],
    content_rating: null,
    cover_url: "",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: {
      status: "reading",
      rating: 3,
      progress_pct: 40,
      started_at: null,
      finished_at: null,
    },
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("title route", () => {
  test("toPatch sends only the title field", () => {
    const route = EDIT_ROUTES.title;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ title: "New Title" });
    expect(route.toPatch(row, draft)).toEqual({ title: "New Title" });
  });
});
