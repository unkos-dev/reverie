import { describe, expect, test } from "vite-plus/test";

import { sortStackSummary } from "./sort-summary";

describe("sortStackSummary", () => {
  test("an empty stack reads as unsorted", () => {
    expect(sortStackSummary([])).toBe("Not sorted");
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
