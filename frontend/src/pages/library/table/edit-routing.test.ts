import { describe, expect, test } from "vite-plus/test";

import type { BookListItem } from "@/api";

import { EDIT_ROUTES } from "./edit-routing";

function rowFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
    reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
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

describe("subtitle route", () => {
  test("toPatch carries null through as a clear", () => {
    const route = EDIT_ROUTES.subtitle;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ subtitle: null });
    expect(route.toPatch(row, draft)).toEqual({ subtitle: null });
  });
});

describe("authors route", () => {
  test("toPatch nests the author list under contributors.author", () => {
    const route = EDIT_ROUTES.authors;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ authors: ["Ann Leckie", "N.K. Jemisin"] });
    expect(route.toPatch(row, draft)).toEqual({
      contributors: { author: ["Ann Leckie", "N.K. Jemisin"] },
    });
  });
});

describe("status route", () => {
  test("routes through the reading pipeline", () => {
    expect(EDIT_ROUTES.status.pipeline).toBe("reading");
  });

  test("toPatch sends only status", () => {
    const route = EDIT_ROUTES.status;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const draft = rowFixture({
      reading_state: {
        status: "finished",
        rating: 3,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    expect(route.toPatch(row, draft)).toEqual({ status: "finished" });
  });
});

describe("rating route", () => {
  test("routes through the reading pipeline", () => {
    expect(EDIT_ROUTES.rating.pipeline).toBe("reading");
  });

  test("toPatch sends only rating", () => {
    const route = EDIT_ROUTES.rating;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const draft = rowFixture({
      reading_state: {
        status: "reading",
        rating: 5,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    expect(route.toPatch(row, draft)).toEqual({ rating: 5 });
  });
});
