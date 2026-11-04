import { expect, test } from "vite-plus/test";
import { buildFilterSummary } from "@/components/library/filter-summary";
import {
  emptyFilterState,
  filterStateToParams,
  hasActiveFilterState,
  LibraryVisitSchema,
} from "./library-params";

test("raw spaces and an operator without text remain visit intent without predicates", () => {
  const filters = { ...emptyFilterState(), q: " ", title: { op: "eq", text: "  " } } as const;
  expect(LibraryVisitSchema.parse({ filters }).filters).toEqual(filters);
  expect(filterStateToParams(filters)).toEqual({});
  expect(
    filterStateToParams({ ...filters, q: " a ", title: { op: "eq", text: " the sea " } }),
  ).toEqual({ q: "a", title_eq: "the sea" });
});

test("separator-only ISBN intent has no active readout while preserving raw requests", () => {
  // Incident: separator-only ISBN searches advertised a predicate the server discarded.
  const filters = {
    ...emptyFilterState(),
    isbn13: { op: "contains", text: " - - " },
  } as const;
  expect(
    buildFilterSummary(filters, {
      authorLabel: (id) => id,
      shelfName: (id) => id,
      seriesName: (id) => id,
    }),
  ).toEqual([]);
  expect(hasActiveFilterState(filters)).toBe(false);
  expect(LibraryVisitSchema.parse({ filters }).filters).toEqual(filters);
  expect(filterStateToParams(filters)).toEqual({ isbn_13_contains: "- -" });
});
