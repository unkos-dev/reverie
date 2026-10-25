import { describe, expect, test } from "vite-plus/test";
import {
  emptyFilterState,
  filterStateToParams,
  hasActiveFilterState,
  LibraryVisitSchema,
  libraryTextError,
  isLibraryRequestEligible,
  TEXT_COLUMN_OPS,
} from "./library-params";

describe("Library visit projection", () => {
  test.each([
    ["a".repeat(200), true],
    ["a".repeat(201), false],
    ["😀".repeat(200), true],
    ["😀".repeat(201), false],
    [`  ${"a".repeat(200)}  `, true],
    [" ".repeat(201), true],
    ["e\u0301".repeat(100), true],
    [`${"e\u0301".repeat(100)}e`, false],
  ])("validates trimmed Unicode scalar values without changing raw intent: %s", (q, eligible) => {
    const filters = { ...emptyFilterState(), q };
    expect(LibraryVisitSchema.parse({ filters }).filters.q).toBe(q);
    expect(isLibraryRequestEligible(filterStateToParams(filters))).toBe(eligible);
    expect(libraryTextError(q) === undefined).toBe(eligible);
  });
  test("rejects the whole active request for each capped text predicate without dropping it", () => {
    const text = "x".repeat(201);
    for (const field of [
      "q",
      "title_contains",
      "title_eq",
      "title_ne",
      "subtitle_contains",
      "isbn_13_contains",
      "isbn_13_eq",
    ] as const) {
      const params = { [field]: text, pages_gte: 100 };
      expect(isLibraryRequestEligible(params)).toBe(false);
      expect(params).toEqual({ [field]: text, pages_gte: 100 });
    }
  });
  test("ignores dormant text under empty operators and revalidates it when active", () => {
    const text = "x".repeat(201);
    const filters = {
      ...emptyFilterState(),
      subtitle: { op: "empty", text },
      isbn13: { op: "empty", text },
    } as const;
    expect(isLibraryRequestEligible(filterStateToParams(filters))).toBe(true);
    expect(filterStateToParams(filters)).toEqual({ subtitle_empty: true, isbn_13_empty: true });
    expect(
      isLibraryRequestEligible(
        filterStateToParams({ ...filters, subtitle: { op: "contains", text } }),
      ),
    ).toBe(false);
  });
  test("empty visits have no effective predicates", () => {
    expect(filterStateToParams(emptyFilterState())).toEqual({});
    expect(hasActiveFilterState(emptyFilterState())).toBe(false);
  });
  test("raw spaces and an operator without text remain visit intent without predicates", () => {
    const filters = { ...emptyFilterState(), q: " ", title: { op: "eq", text: "  " } } as const;
    expect(LibraryVisitSchema.parse({ filters }).filters).toEqual(filters);
    expect(filterStateToParams(filters)).toEqual({});
    expect(
      filterStateToParams({ ...filters, q: " a ", title: { op: "eq", text: " the sea " } }),
    ).toEqual({ q: "a", title_eq: "the sea" });
  });
  test.each([
    ["title", "title"],
    ["subtitle", "subtitle"],
    ["isbn13", "isbn_13"],
  ] as const)("projects the supported %s operators", (prop, column) => {
    for (const op of TEXT_COLUMN_OPS[column]) {
      const filters = { ...emptyFilterState(), [prop]: { op, text: "needle" } };
      expect(filterStateToParams(filters)).toEqual({
        [`${column}_${op}`]: op === "empty" ? true : "needle",
      });
    }
  });
  test("sets, statuses, identifiers and contradictory bounds reach the API unchanged", () => {
    const filters = emptyFilterState();
    filters.pages = { gte: 400, lte: 200 };
    filters.rating = { gte: 5, lte: 1 };
    filters.authors = { all: ["a"], any: ["b"], none: ["c"] };
    filters.tags = { all: ["fantasy"], any: ["classic"], none: ["grim"] };
    filters.status = { any: ["reading"], none: ["abandoned"] };
    filters.series = "series";
    filters.shelf = "shelf";
    expect(filterStateToParams(filters)).toEqual({
      pages_gte: 400,
      pages_lte: 200,
      rating_gte: 5,
      rating_lte: 1,
      author: ["a"],
      author_any: ["b"],
      author_none: ["c"],
      tag: ["fantasy"],
      tag_any: ["classic"],
      tag_none: ["grim"],
      status_any: ["reading"],
      status_none: ["abandoned"],
      series: "series",
      shelf: "shelf",
    });
  });
  test.each([
    ["Australia/Perth", "2026-10-25", "2026-10-24T16:00:00.000Z", "2026-10-25T16:00:00.000Z"],
    ["America/New_York", "2026-03-08", "2026-03-08T05:00:00.000Z", "2026-03-09T04:00:00.000Z"],
    ["America/New_York", "2026-11-01", "2026-11-01T04:00:00.000Z", "2026-11-02T05:00:00.000Z"],
    ["Australia/Perth", "2026-12-31", "2026-12-30T16:00:00.000Z", "2026-12-31T16:00:00.000Z"],
    ["Australia/Perth", "2028-02-29", "2028-02-28T16:00:00.000Z", "2028-02-29T16:00:00.000Z"],
  ])("projects local calendar days in %s on %s", (zone, day, lower, upper) => {
    const state = { ...emptyFilterState(), addedAfter: day, addedBefore: day };
    expect(filterStateToParams(state, zone)).toEqual({
      created_at_gte: lower,
      created_at_lt: upper,
    });
    expect(filterStateToParams({ ...state, addedBefore: undefined }, zone)).toEqual({
      created_at_gte: lower,
    });
    expect(filterStateToParams({ ...state, addedAfter: undefined }, zone)).toEqual({
      created_at_lt: upper,
    });
  });
  test.each([
    { pages: { gte: -1 } },
    { pages: { lte: 2147483648 } },
    { pages: { gte: 1.5 } },
    { rating: { gte: 0 } },
    { rating: { lte: 6 } },
    { addedAfter: "2026-02-30" },
    { addedAfter: "2026-01-01T00:00:00Z" },
    { status: { any: ["bogus"], none: [] } },
    { title: { op: "empty" } },
    { shelf: "not-a-uuid" },
  ])("rejects an unusable owned visit as a unit: %j", (patch) => {
    expect(
      LibraryVisitSchema.safeParse({ filters: { ...emptyFilterState(), ...patch } }).success,
    ).toBe(false);
  });
  test("accepts independent endpoints, optional view and valid serialisable calendar values", () => {
    const filters = {
      ...emptyFilterState(),
      addedAfter: "2026-10-25",
      addedBefore: "2026-01-01",
      pages: { gte: 400, lte: 200 },
    };
    expect(LibraryVisitSchema.parse({ filters, view: "table" })).toEqual({
      filters,
      view: "table",
    });
    expect(LibraryVisitSchema.safeParse({ filters, view: "list" }).success).toBe(false);
  });
});
