import { describe, expect, test } from "vite-plus/test";

import { formatCount, pluralise, splitPath } from "./format";

describe("ingestion formatting", () => {
  test("pluralise uses the singular only for one", () => {
    expect(pluralise(0, "file")).toBe("0 files");
    expect(pluralise(1, "file")).toBe("1 file");
    expect(pluralise(2, "file")).toBe("2 files");
    expect(pluralise(1, "time")).toBe("1 time");
  });

  test("formatCount groups thousands", () => {
    expect(formatCount(1204)).toBe("1,204");
  });

  test("splitPath separates the directory prefix from the file name", () => {
    expect(splitPath("incoming/older/Dunmore.epub")).toEqual({
      dir: "incoming/older/",
      name: "Dunmore.epub",
    });
    expect(splitPath("Loose.epub")).toEqual({ dir: "", name: "Loose.epub" });
  });
});
