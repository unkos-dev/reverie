import { describe, expect, test } from "vite-plus/test";

import { parseFilterParams } from "@/routes/library-params";

import { buildFilterSummary } from "./filter-summary";

const RESOLVERS = {
  authorLabel: (id: string) => id,
  shelfName: (id: string) => id,
  seriesName: (id: string) => id,
};

/** Every filter family active at once, so each segment's removal can be
 *  checked against the full set of surviving siblings. */
const KITCHEN_SINK = new URLSearchParams(
  [
    "q=dune",
    "shelf=shelf-1",
    "series=s-1",
    "author=a-1",
    "author_any=a-2",
    "author_none=a-3",
    "tag=scifi",
    "genre_any=gothic",
    "mood_none=grim",
    "status_any=reading",
    "status_none=abandoned",
    "title_contains=sea",
    "title_eq=Dune",
    "title_ne=Emma",
    "subtitle_contains=voyage",
    "subtitle_empty=true",
    "isbn_13_contains=978",
    "isbn_13_eq=9780000000000",
    "isbn_13_empty=false",
    "pages_gte=100",
    "pages_lte=900",
    "pages_empty=false",
    "rating_gte=2",
    "rating_lte=4",
    "created_at_gte=2026-01-01",
    "created_at_lte=2026-06-30",
  ].join("&"),
);

describe("buildFilterSummary removal patches", () => {
  test("the kitchen-sink state produces a segment per active family and operator", () => {
    const segments = buildFilterSummary(parseFilterParams(KITCHEN_SINK), RESOLVERS);
    expect(segments.map((segment) => segment.key).sort()).toEqual(
      [
        "q",
        "shelf",
        "series",
        "authors",
        "tags",
        "genres",
        "moods",
        "status",
        "title_contains",
        "title_eq",
        "title_ne",
        "subtitle_contains",
        "subtitle_empty",
        "isbn_contains",
        "isbn_eq",
        "isbn_empty",
        "pages_gte",
        "pages_lte",
        "pages_empty",
        "rating_gte",
        "rating_lte",
        "added_after",
        "added_before",
      ].sort(),
    );
  });

  test("each segment's removal drops exactly that segment and preserves every sibling", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const segments = buildFilterSummary(state, RESOLVERS);
    for (const segment of segments) {
      const nextKeys = buildFilterSummary(segment.remove(state), RESOLVERS).map((next) => next.key);
      const expected = segments.map((s) => s.key).filter((key) => key !== segment.key);
      expect(nextKeys, `removing ${segment.key}`).toEqual(expected);
    }
  });

  test("removal patches never mutate the input state", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const before = JSON.stringify(state);
    for (const segment of buildFilterSummary(state, RESOLVERS)) {
      segment.remove(state);
    }
    expect(JSON.stringify(state)).toBe(before);
  });

  test("a vocabulary family's removal clears all three modes at once", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const authors = buildFilterSummary(state, RESOLVERS).find((s) => s.key === "authors");
    if (authors === undefined) throw new Error("expected an authors segment");
    const next = authors.remove(state);
    expect(next.authors).toEqual({ all: [], any: [], none: [] });
    // Sibling vocab families are untouched.
    expect(next.tags.all).toEqual(["scifi"]);
    expect(next.genres.any).toEqual(["gothic"]);
    expect(next.moods.none).toEqual(["grim"]);
  });

  test("a status removal clears include and exclude modes together", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const status = buildFilterSummary(state, RESOLVERS).find((s) => s.key === "status");
    if (status === undefined) throw new Error("expected a status segment");
    expect(status.remove(state).status).toEqual({ any: [], none: [] });
  });

  test("a text-operator removal keeps the column's other operators", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const titleEq = buildFilterSummary(state, RESOLVERS).find((s) => s.key === "title_eq");
    if (titleEq === undefined) throw new Error("expected a title_eq segment");
    const next = titleEq.remove(state);
    expect(next.title.eq).toBeUndefined();
    expect(next.title.contains).toBe("sea");
    expect(next.title.ne).toBe("Emma");
  });

  test("a range-operator removal keeps the column's other bound", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const pagesGte = buildFilterSummary(state, RESOLVERS).find((s) => s.key === "pages_gte");
    if (pagesGte === undefined) throw new Error("expected a pages_gte segment");
    const next = pagesGte.remove(state);
    expect(next.pages.gte).toBeUndefined();
    expect(next.pages.lte).toBe(900);
  });

  test("date-bound removals are independent", () => {
    const state = parseFilterParams(KITCHEN_SINK);
    const after = buildFilterSummary(state, RESOLVERS).find((s) => s.key === "added_after");
    if (after === undefined) throw new Error("expected an added_after segment");
    const next = after.remove(state);
    expect(next.addedAfter).toBeUndefined();
    expect(next.addedBefore).toBe("2026-06-30");
  });
});
