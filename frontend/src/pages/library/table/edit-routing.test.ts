import { describe, expect, test } from "vite-plus/test";

import type { BookListItem } from "@/api";
import type { FieldVersionChange } from "@/api/books";

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

function fieldChange(value: unknown): FieldVersionChange {
  return {
    value,
    version_id: "11111111-1111-1111-1111-111111111111",
    previous_version_id: "00000000-0000-0000-0000-000000000000",
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

describe("isbn_13 route", () => {
  test("applyToRow applies the server-normalized ISBN, not the draft", () => {
    const route = EDIT_ROUTES.isbn_13;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    // Draft carries hyphenated input; the server echoes back the
    // checksum-normalized canonical value, which is what must win.
    const draft = rowFixture({ isbn_13: "978-0-00-000000-1" });
    expect(route.toPatch(row, draft)).toEqual({ isbn_13: "978-0-00-000000-1" });
    const applied = route.applyToRow(row, fieldChange("9780000000001"));
    expect(applied.isbn_13).toBe("9780000000001");
  });
});

describe("pages route", () => {
  test("toPatch and applyToRow round-trip a number", () => {
    const route = EDIT_ROUTES.pages;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ pages: 250 });
    expect(route.toPatch(row, draft)).toEqual({ pages: 250 });
    const applied = route.applyToRow(row, fieldChange(250));
    expect(applied.pages).toBe(250);
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
