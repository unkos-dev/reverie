import { describe, expect, test } from "vite-plus/test";

import { SpikeBookRowSchema } from "../types";
import { DEFAULT_ROW_COUNT, generateRows } from "./generator";

describe("generateRows", () => {
  test("same seed yields byte-identical rows", () => {
    const a = generateRows(1234, 500);
    const b = generateRows(1234, 500);
    expect(a).toEqual(b);
  });

  test("different seeds diverge", () => {
    const a = generateRows(1, 200);
    const b = generateRows(2, 200);
    expect(a).not.toEqual(b);
  });

  test("prefix stability: a larger count extends, never reshuffles", () => {
    const small = generateRows(77, 100);
    const large = generateRows(77, 300);
    expect(large.slice(0, 100)).toEqual(small);
  });

  test("row count and default match request", () => {
    expect(generateRows(9, 42)).toHaveLength(42);
    expect(DEFAULT_ROW_COUNT).toBe(50_000);
  });

  test("every row satisfies the SpikeBookRow schema", () => {
    for (const row of generateRows(5, 250)) {
      expect(() => SpikeBookRowSchema.parse(row)).not.toThrow();
    }
  });

  test("ids are unique and index-stable", () => {
    const rows = generateRows(3, 1000);
    const ids = new Set(rows.map((r) => r.id));
    expect(ids.size).toBe(1000);
    expect(rows[0]?.id).toBe("00000000-0000-4000-8000-000000000000");
    expect(rows[999]?.id).toBe("00000000-0000-4000-8000-0000000003e7");
  });
});
