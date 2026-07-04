import { describe, expect, test } from "vite-plus/test";

import { percentile, summarize } from "./instrument";

describe("percentile", () => {
  test("empty sample is zero", () => {
    expect(percentile([], 95)).toBe(0);
  });

  test("nearest-rank picks the expected element", () => {
    const s = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    expect(percentile(s, 50)).toBe(5);
    expect(percentile(s, 95)).toBe(10);
    expect(percentile(s, 100)).toBe(10);
  });

  test("single sample returns that value for any percentile", () => {
    expect(percentile([42], 50)).toBe(42);
    expect(percentile([42], 95)).toBe(42);
  });
});

describe("summarize", () => {
  test("empty set is all zeros", () => {
    expect(summarize([])).toEqual({ count: 0, p50: 0, p95: 0, max: 0 });
  });

  test("reports count, p50, p95 and max regardless of input order", () => {
    const unordered = [10, 1, 5, 3, 8, 2, 9, 4, 7, 6];
    const s = summarize(unordered);
    expect(s.count).toBe(10);
    expect(s.p50).toBe(5);
    expect(s.p95).toBe(10);
    expect(s.max).toBe(10);
  });
});
