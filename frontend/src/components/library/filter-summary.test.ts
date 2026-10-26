import { describe, expect, test } from "vite-plus/test";
import { emptyFilterState, type FilterState } from "@/routes/library-params";
import { buildFilterSummary } from "./filter-summary";

const RESOLVERS = {
  authorLabel: (id: string) => id,
  shelfName: (id: string) => id,
  seriesName: (id: string) => id,
};
const state: FilterState = {
  ...emptyFilterState(),
  q: " dune ",
  shelf: "shelf",
  series: "series",
  authors: { all: ["a"], any: ["b"], none: ["c"] },
  tags: { all: ["scifi"], any: [], none: [] },
  genres: { all: [], any: ["gothic"], none: [] },
  moods: { all: [], any: [], none: ["grim"] },
  status: { any: ["reading"], none: ["abandoned"] },
  title: { op: "eq", text: " Dune " },
  subtitle: { op: "empty", text: "retained" },
  isbn13: { op: "contains", text: "978" },
  pages: { gte: 100, lte: 900, empty: false },
  rating: { gte: 2, lte: 4 },
  addedAfter: "2026-01-01",
  addedBefore: "2026-06-30",
};

describe("filter summary", () => {
  test("each removal drops exactly its effective predicate and preserves siblings without mutation", () => {
    const before = JSON.stringify(state);
    const segments = buildFilterSummary(state, RESOLVERS);
    expect(segments.map((segment) => segment.key)).toEqual([
      "q",
      "shelf",
      "series",
      "authors",
      "tags",
      "genres",
      "moods",
      "status",
      "title_eq",
      "subtitle_empty",
      "isbn_contains",
      "pages_gte",
      "pages_lte",
      "pages_empty",
      "rating_gte",
      "rating_lte",
      "added_after",
      "added_before",
    ]);
    for (const segment of segments) {
      expect(buildFilterSummary(segment.remove(state), RESOLVERS).map((next) => next.key)).toEqual(
        segments.filter((next) => next.key !== segment.key).map((next) => next.key),
      );
    }
    expect(JSON.stringify(state)).toBe(before);
  });
  test("operator-only and whitespace conditions have no chips", () => {
    expect(
      buildFilterSummary(
        { ...emptyFilterState(), q: " ", title: { op: "eq", text: " " } },
        RESOLVERS,
      ),
    ).toEqual([]);
  });
  test("readouts describe exclusion and included local days", () => {
    const words = buildFilterSummary(state, RESOLVERS).map((segment) => segment.text);
    expect(words).toContain("Mood: not grim");
    expect(words).toContain('Title is "Dune"');
    expect(words).toContain("Added on or after 2026-01-01");
    expect(words).toContain("Added on or before 2026-06-30");
  });
});
