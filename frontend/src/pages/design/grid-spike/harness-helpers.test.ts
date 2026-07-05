import { describe, expect, test } from "vite-plus/test";

import { recordMove } from "./harness-helpers";

describe("recordMove", () => {
  test("records a resolved move's delta and does not flag a drop", () => {
    const samples: number[] = [];
    const dropped = recordMove(samples, 12.5, 300);
    expect(samples).toEqual([12.5]);
    expect(dropped).toBe(false);
  });

  test("records the timeout ceiling and flags a drop when focus never arrives", () => {
    const samples: number[] = [];
    const dropped = recordMove(samples, null, 300);
    expect(samples).toEqual([300]);
    expect(dropped).toBe(true);
  });

  test("a stalled candidate cannot hide drops behind a small sample", () => {
    const samples: number[] = [];
    let dropped = 0;
    for (const delta of [10, null, 12, null, null]) {
      if (recordMove(samples, delta, 300)) dropped += 1;
    }
    expect(samples).toHaveLength(5);
    expect(dropped).toBe(3);
    expect(Math.max(...samples)).toBe(300);
  });
});
