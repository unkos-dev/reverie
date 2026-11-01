import { expect, test } from "vite-plus/test";
import { emptyFilterState, filterStateToParams, LibraryVisitSchema } from "./library-params";

test("raw spaces and an operator without text remain visit intent without predicates", () => {
  const filters = { ...emptyFilterState(), q: " ", title: { op: "eq", text: "  " } } as const;
  expect(LibraryVisitSchema.parse({ filters }).filters).toEqual(filters);
  expect(filterStateToParams(filters)).toEqual({});
  expect(
    filterStateToParams({ ...filters, q: " a ", title: { op: "eq", text: " the sea " } }),
  ).toEqual({ q: "a", title_eq: "the sea" });
});
