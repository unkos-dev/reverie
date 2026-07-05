import { describe, expect, test } from "vite-plus/test";

import type { SortState } from "@/lib/grid/types";

import type { SpikeBookRow } from "../types";
import { generateRows } from "./generator";
import { listSpikeBooks, MockCursorError } from "./mock-api";

const DATA = generateRows(42, 205);

async function collectAll(
  dataset: readonly SpikeBookRow[],
  pageSize: number,
  sort: SortState = null,
): Promise<SpikeBookRow[]> {
  const all: SpikeBookRow[] = [];
  let cursor: string | undefined;
  for (;;) {
    const page = await listSpikeBooks(dataset, { cursor, pageSize, sort, latencyMs: 0 });
    all.push(...page.items);
    if (page.next_cursor === null) break;
    cursor = page.next_cursor;
  }
  return all;
}

describe("listSpikeBooks paging", () => {
  test("first page returns pageSize items and a continuation cursor", async () => {
    const page = await listSpikeBooks(DATA, { pageSize: 50, sort: null, latencyMs: 0 });
    expect(page.items).toHaveLength(50);
    expect(page.next_cursor).not.toBeNull();
  });

  test("walking the cursor covers every row exactly once, in order", async () => {
    const all = await collectAll(DATA, 50);
    expect(all).toHaveLength(DATA.length);
    expect(all.map((r) => r.id)).toEqual(DATA.map((r) => r.id));
  });

  test("final partial page is short and terminates the cursor", async () => {
    // 205 rows / 50 => pages of 50,50,50,50,5.
    let cursor: string | undefined;
    let last = await listSpikeBooks(DATA, { pageSize: 50, sort: null, latencyMs: 0 });
    while (last.next_cursor !== null) {
      cursor = last.next_cursor;
      last = await listSpikeBooks(DATA, { cursor, pageSize: 50, sort: null, latencyMs: 0 });
    }
    expect(last.items).toHaveLength(5);
    expect(last.next_cursor).toBeNull();
  });

  test("an empty dataset returns an empty first page and no cursor", async () => {
    const page = await listSpikeBooks([], { pageSize: 50, sort: null, latencyMs: 0 });
    expect(page.items).toHaveLength(0);
    expect(page.next_cursor).toBeNull();
  });
});

describe("listSpikeBooks sort", () => {
  test("ascending title sort is monotonic across the whole set", async () => {
    const all = await collectAll(DATA, 40, { columnKey: "title", direction: "asc" });
    const titles = all.map((r) => r.title);
    const sorted = [...titles].sort((a, b) => a.localeCompare(b));
    expect(titles).toEqual(sorted);
  });

  test("descending pages sort puts nulls last within the ordering", async () => {
    const page = await listSpikeBooks(DATA, {
      pageSize: DATA.length,
      sort: { columnKey: "pages", direction: "desc" },
      latencyMs: 0,
    });
    const pages = page.items.map((r) => r.pages);
    const firstNull = pages.indexOf(null);
    const nonNull = firstNull === -1 ? pages : pages.slice(0, firstNull);
    const tail = firstNull === -1 ? [] : pages.slice(firstNull);
    expect(tail.every((p) => p === null)).toBe(true);
    for (let i = 1; i < nonNull.length; i += 1) {
      expect(Number(nonNull[i - 1])).toBeGreaterThanOrEqual(Number(nonNull[i]));
    }
  });
});

describe("listSpikeBooks cursor edges", () => {
  test("a malformed cursor throws MockCursorError", async () => {
    await expect(
      listSpikeBooks(DATA, { cursor: "!!!not-base64!!!", pageSize: 10, sort: null, latencyMs: 0 }),
    ).rejects.toBeInstanceOf(MockCursorError);
  });

  test("a well-formed but out-of-range offset yields an empty terminal page", async () => {
    const beyond = btoa("offset:99999");
    const page = await listSpikeBooks(DATA, {
      cursor: beyond,
      pageSize: 10,
      sort: null,
      latencyMs: 0,
    });
    expect(page.items).toHaveLength(0);
    expect(page.next_cursor).toBeNull();
  });
});

describe("listSpikeBooks latency", () => {
  test("aborting a latent request rejects with AbortError", async () => {
    const controller = new AbortController();
    const pending = listSpikeBooks(
      DATA,
      { pageSize: 10, sort: null, latencyMs: 200 },
      controller.signal,
    );
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
  });
});
