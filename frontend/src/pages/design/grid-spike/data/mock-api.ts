/**
 * In-memory mock of the `GET /api/v1/books` list contract (design D3).
 *
 * No backend contact: the harness feeds a pre-generated dataset and this
 * module paginates over it with cursor + page-size + sort, plus a latency knob
 * for exercising loading states. The response is parsed through a Zod schema
 * on the way out, mirroring the real `listBooks` boundary so the harness can
 * consume it through the identical `useSuspenseInfiniteQuery` shape
 * `LibraryPage` uses.
 *
 * Server-side list p95 is deliberately unmeasured this phase; a later phase
 * drives the real endpoint.
 */
import { z } from "zod";

import type { SortState } from "@/lib/grid/types";

import { SpikeBookRowSchema, type SpikeBookRow } from "../types";

const SpikeListResponseSchema = z.object({
  items: z.array(SpikeBookRowSchema),
  next_cursor: z.string().nullable(),
});
export type SpikeListResponse = z.infer<typeof SpikeListResponseSchema>;

/** Latency presets the harness exposes (design D5 loading states). */
export const LATENCY_PRESETS = [0, 50, 200] as const;
export type LatencyMs = (typeof LATENCY_PRESETS)[number];

export type MockListParams = {
  cursor?: string;
  pageSize: number;
  sort: SortState;
  latencyMs: number;
};

/** Raised for a malformed cursor, mirroring a real 400 rather than silent reset. */
export class MockCursorError extends Error {
  constructor(cursor: string) {
    super(`invalid cursor: ${cursor}`);
    this.name = "MockCursorError";
  }
}

/** Opaque offset cursor. Base64 keeps callers from depending on the format. */
function encodeCursor(offset: number): string {
  return btoa(`offset:${String(offset)}`);
}

function decodeCursor(cursor: string): number {
  let decoded: string;
  try {
    decoded = atob(cursor);
  } catch {
    throw new MockCursorError(cursor);
  }
  const match = /^offset:(\d+)$/.exec(decoded);
  if (match === null) throw new MockCursorError(cursor);
  return Number(match[1]);
}

function compareStrings(a: string, b: string): number {
  return a.localeCompare(b);
}

/**
 * Comparable value for a column; `null` sorts last regardless of direction.
 * `columnKey` is the promoted contract's plain `string` (not a harness-local
 * enum), so unrecognized keys fall through to `null` rather than requiring
 * exhaustiveness.
 */
function sortValue(columnKey: string, row: SpikeBookRow): string | number | null {
  switch (columnKey) {
    case "pages":
      return row.pages;
    case "authors":
      return row.authors.join(", ");
    case "subtitle":
      return row.subtitle;
    case "isbn_13":
      return row.isbn_13;
    case "reading_state":
      return row.reading_state === null ? null : row.reading_state.status;
    case "title":
      return row.title;
    default:
      return null;
  }
}

/**
 * Field comparator. Nulls always sort last (independent of direction), then the
 * non-null values compare in `direction`, with a stable `id` tiebreak.
 */
function comparator(sort: NonNullable<SortState>): (a: SpikeBookRow, b: SpikeBookRow) => number {
  const dir = sort.direction === "asc" ? 1 : -1;
  return (a, b) => {
    const av = sortValue(sort.columnKey, a);
    const bv = sortValue(sort.columnKey, b);
    if (av === null && bv === null) return compareStrings(a.id, b.id);
    if (av === null) return 1;
    if (bv === null) return -1;
    const primary =
      typeof av === "number" && typeof bv === "number"
        ? av - bv
        : compareStrings(String(av), String(bv));
    if (primary !== 0) return primary * dir;
    return compareStrings(a.id, b.id);
  };
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  if (ms <= 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(new DOMException("Aborted", "AbortError"));
      },
      { once: true },
    );
  });
}

/**
 * Sorted-view cache, one entry deep. A fetch-on-scroll walk issues many
 * sequential pages over one (dataset, sort) pair; re-sorting 50K rows per
 * page would bill O(n log n) mock work against the latency numbers the
 * harness exists to isolate. The real endpoint pays sort cost once per
 * page inside the measured server time, so the mock must not multiply it.
 */
let sortCache: {
  dataset: readonly SpikeBookRow[];
  sort: NonNullable<MockListParams["sort"]>;
  ordered: readonly SpikeBookRow[];
} | null = null;

function sortedView(
  dataset: readonly SpikeBookRow[],
  sort: MockListParams["sort"],
): readonly SpikeBookRow[] {
  if (sort === null) return dataset;
  if (
    sortCache !== null &&
    sortCache.dataset === dataset &&
    sortCache.sort.columnKey === sort.columnKey &&
    sortCache.sort.direction === sort.direction
  ) {
    return sortCache.ordered;
  }
  const ordered = [...dataset].sort(comparator(sort));
  sortCache = { dataset, sort, ordered };
  return ordered;
}

/**
 * Return one page of the sorted dataset starting at `cursor`. A `null`
 * `next_cursor` marks end-of-list; a partial final page returns fewer than
 * `pageSize` items and a `null` cursor.
 */
export async function listSpikeBooks(
  dataset: readonly SpikeBookRow[],
  params: MockListParams,
  signal?: AbortSignal,
): Promise<SpikeListResponse> {
  await sleep(params.latencyMs, signal);

  const offset = params.cursor === undefined ? 0 : decodeCursor(params.cursor);
  const ordered = sortedView(dataset, params.sort);

  const items = ordered.slice(offset, offset + params.pageSize);
  const nextOffset = offset + items.length;
  const next_cursor = nextOffset < ordered.length ? encodeCursor(nextOffset) : null;

  return SpikeListResponseSchema.parse({ items, next_cursor });
}
