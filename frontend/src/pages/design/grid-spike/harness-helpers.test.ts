import { describe, expect, test } from "vite-plus/test";

import { applyEdits, recordMove, type RowEdits } from "./harness-helpers";
import type { SpikeBookRow } from "./types";

function row(id: string, title: string): SpikeBookRow {
  return {
    id,
    title,
    subtitle: null,
    authors: ["Author"],
    isbn_13: null,
    pages: null,
    reading_state: null,
  };
}

describe("applyEdits", () => {
  test("returns the same reference when there are no edits", () => {
    const rows = [row("a", "Original")];
    expect(applyEdits(rows, new Map())).toBe(rows);
  });

  test("overlays a committed title without touching other rows or fields", () => {
    const rows = [row("a", "Original A"), row("b", "Original B")];
    const edits: RowEdits = new Map([["a", "Edited A"]]);

    const result = applyEdits(rows, edits);

    expect(result[0]?.title).toBe("Edited A");
    expect(result[0]?.authors).toEqual(["Author"]);
    expect(result[1]?.title).toBe("Original B");
  });

  test("ignores edits for rows absent from the window", () => {
    const rows = [row("a", "Original A")];
    const result = applyEdits(rows, new Map([["missing", "ghost"]]));
    expect(result.map((r) => r.title)).toEqual(["Original A"]);
  });
});

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
