import { describe, expect, test, vi } from "vite-plus/test";

import type { BookListItem } from "@/api";
import type { FieldVersionChange } from "@/api/books";
import type { ReadingState } from "@/api/reading";

import { EDIT_ROUTES, type EditableColumnKey } from "./edit-routing";

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
      notes: null,
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

function readingStateFixture(overrides: Partial<ReadingState> = {}): ReadingState {
  return {
    status: "reading",
    rating: 3,
    notes: null,
    progress_pct: 40,
    started_at: null,
    finished_at: null,
    last_read_at: null,
    ...overrides,
  };
}

describe("EDIT_ROUTES registry", () => {
  test("covers exactly the editable columns, series excluded", () => {
    const expected: EditableColumnKey[] = [
      "title",
      "subtitle",
      "isbn_13",
      "pages",
      "authors",
      "status",
      "rating",
    ];
    expect(Object.keys(EDIT_ROUTES).sort()).toEqual([...expected].sort());
  });
});

describe.each([
  { key: "title" as const, field: "title", workScoped: true },
  { key: "subtitle" as const, field: "subtitle", workScoped: true },
  { key: "isbn_13" as const, field: "isbn_13", workScoped: false },
  { key: "pages" as const, field: "pages", workScoped: false },
  { key: "authors" as const, field: "contributors.author", workScoped: true },
])("$key column", ({ key, field, workScoped }) => {
  test("routes through the metadata pipeline with the declared field and work scope", () => {
    const route = EDIT_ROUTES[key];
    expect(route.pipeline).toBe("metadata");
    if (route.pipeline !== "metadata") return;
    expect(route.field).toBe(field);
    expect(route.workScoped).toBe(workScoped);
  });
});

describe("title route", () => {
  test("toPatch sends only the title field", () => {
    const route = EDIT_ROUTES.title;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ title: "New Title" });
    expect(route.toPatch(row, draft)).toEqual({ title: "New Title" });
  });

  test("applyToRow replaces title with the server-normalized value, immutably", () => {
    const route = EDIT_ROUTES.title;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange("Server Title"));
    expect(applied.title).toBe("Server Title");
    expect(applied).not.toBe(row);
    expect(row.title).toBe("Original Title");
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

  test("applyToRow clears subtitle when the server echoes null", () => {
    const route = EDIT_ROUTES.subtitle;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(null));
    expect(applied.subtitle).toBeNull();
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

  test("clearing pages sends and applies null", () => {
    const route = EDIT_ROUTES.pages;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const draft = rowFixture({ pages: null });
    expect(route.toPatch(row, draft)).toEqual({ pages: null });
    const applied = route.applyToRow(row, fieldChange(null));
    expect(applied.pages).toBeNull();
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

  test("applyToRow replaces the authors array from the response value", () => {
    const route = EDIT_ROUTES.authors;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(["Ann Leckie"]));
    expect(applied.authors).toEqual(["Ann Leckie"]);
    expect(row.authors).toEqual(["Original Author"]);
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
        notes: null,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    expect(route.toPatch(row, draft)).toEqual({ status: "finished" });
  });

  test("null reading_state on the draft clears status in the patch", () => {
    const route = EDIT_ROUTES.status;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const draft = rowFixture({ reading_state: null });
    expect(route.toPatch(row, draft)).toEqual({ status: null });
  });

  test("applyToRow patches the reading_state summary subset from the full response", () => {
    const route = EDIT_ROUTES.status;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const response = readingStateFixture({
      status: "finished",
      rating: 3,
      progress_pct: 100,
      finished_at: "2026-07-01T00:00:00Z",
    });
    const applied = route.applyToRow(row, response);
    expect(applied.reading_state).toEqual({
      status: "finished",
      rating: 3,
      notes: null,
      progress_pct: 100,
      started_at: null,
      finished_at: "2026-07-01T00:00:00Z",
    });
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
        notes: null,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    expect(route.toPatch(row, draft)).toEqual({ rating: 5 });
  });

  test("clearing rating on the draft sends null", () => {
    const route = EDIT_ROUTES.rating;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const draft = rowFixture({ reading_state: null });
    expect(route.toPatch(row, draft)).toEqual({ rating: null });
  });

  test("applyToRow patches the reading_state summary subset from the full response", () => {
    const route = EDIT_ROUTES.rating;
    if (route.pipeline !== "reading") throw new Error("expected reading pipeline");
    const row = rowFixture();
    const response = readingStateFixture({ rating: 1, progress_pct: 40 });
    const applied = route.applyToRow(row, response);
    expect(applied.reading_state).toEqual({
      status: "reading",
      rating: 1,
      notes: null,
      progress_pct: 40,
      started_at: null,
      finished_at: null,
    });
  });
});

describe("coercion fallback logging", () => {
  test("an unexpected non-string title value logs and falls back to the row's own title", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.title;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(42));
    expect(applied.title).toBe(row.title);
    expect(errorSpy).toHaveBeenCalledWith(
      "[edit-routing] unexpected applied-value type for title",
      42,
    );
    errorSpy.mockRestore();
  });

  test("a null subtitle value clears the field without logging", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.subtitle;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(null));
    expect(applied.subtitle).toBeNull();
    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  test("an unexpected non-string subtitle value logs and falls back to null", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.subtitle;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(42));
    expect(applied.subtitle).toBeNull();
    expect(errorSpy).toHaveBeenCalledWith(
      "[edit-routing] unexpected applied-value type for subtitle",
      42,
    );
    errorSpy.mockRestore();
  });

  test("an unexpected non-number pages value logs and falls back to null", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.pages;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange("not-a-number"));
    expect(applied.pages).toBeNull();
    expect(errorSpy).toHaveBeenCalledWith(
      "[edit-routing] unexpected applied-value type for pages",
      "not-a-number",
    );
    errorSpy.mockRestore();
  });

  test("a null authors value clears the list without logging", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.authors;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange(null));
    expect(applied.authors).toEqual([]);
    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  test("an unexpected non-array authors value logs and falls back to an empty list", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const route = EDIT_ROUTES.authors;
    if (route.pipeline !== "metadata") throw new Error("expected metadata pipeline");
    const row = rowFixture();
    const applied = route.applyToRow(row, fieldChange("not-an-array"));
    expect(applied.authors).toEqual([]);
    expect(errorSpy).toHaveBeenCalledWith(
      "[edit-routing] unexpected applied-value type for contributors.author",
      "not-an-array",
    );
    errorSpy.mockRestore();
  });
});
