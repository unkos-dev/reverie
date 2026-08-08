import { describe, expect, test } from "vite-plus/test";

import { sortStackSummary } from "./sort-summary";

describe("sortStackSummary", () => {
  test("an empty stack names the installation order, never an unsorted state", () => {
    // The library always has a total order; an empty stack only means the
    // specific levels are not yet known (pre-response, or a failed
    // preferences endpoint), so the summary must not claim otherwise.
    expect(sortStackSummary([])).toBe("Sorted by the installation order");
  });

  test("a single level names the field and direction", () => {
    expect(sortStackSummary([{ field: "pages", desc: true }])).toBe("Sorted by Pages descending");
  });

  test("multiple levels join in priority order", () => {
    expect(
      sortStackSummary([
        { field: "author", desc: false },
        { field: "created_at", desc: true },
      ]),
    ).toBe("Sorted by Authors ascending, then Added descending");
  });
});
