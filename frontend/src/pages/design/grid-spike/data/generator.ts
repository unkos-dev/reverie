/**
 * Deterministic synthetic-library generator (design D3).
 *
 * A seeded PRNG produces the row set, so the same seed yields byte-identical
 * rows on every run and perf numbers reproduce across sessions and machines.
 * Rows match the `BookListRow` projection via `SpikeBookRow`.
 */
import type { ReadingStateSummary, SpikeBookRow } from "../types";

/** Default synthetic library size for the 50K-row budget runs. */
export const DEFAULT_ROW_COUNT = 50_000;

/**
 * mulberry32: a small, fast, well-distributed 32-bit PRNG. Deterministic for a
 * given seed, which is the whole point; `Math.random` is banned here because it
 * would break reproducibility (and the cardinal no-`Math.random` rule).
 */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return function next(): number {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

const TITLE_HEAD = [
  "The Silent",
  "A Study in",
  "Echoes of",
  "The Last",
  "Notes on",
  "The Weight of",
  "Under the",
  "Fragments of",
  "The Book of",
  "Letters to",
];
const TITLE_TAIL = [
  "Cartographers",
  "Winter",
  "the Deep",
  "Machine",
  "Provenance",
  "Ash",
  "Meridian",
  "the Archive",
  "Small Gods",
  "Tomorrow",
];
const SUBTITLES = ["A Novel", "And Other Stories", "Volume One", "An Investigation", "A Memoir"];
const GIVEN = ["Ada", "Boris", "Chen", "Dara", "Emil", "Fenna", "Goro", "Hana", "Ivo", "Juno"];
const FAMILY = [
  "Okafor",
  "Lindqvist",
  "Nakamura",
  "Delacroix",
  "Vasquez",
  "Novak",
  "Haddad",
  "Petrov",
  "Sørensen",
  "Ferrara",
];
const READING_STATUSES = ["unread", "reading", "finished", "abandoned"];

function pick<T>(rng: () => number, pool: readonly T[]): T {
  const value = pool[Math.floor(rng() * pool.length)];
  // `pool` is always non-empty here; the index is in range by construction.
  if (value === undefined) throw new Error("empty pool passed to pick");
  return value;
}

/** Deterministic pseudo-UUID from a row index; stable, not RFC-4122 valid. */
function rowId(index: number): string {
  const hex = (index >>> 0).toString(16).padStart(12, "0");
  return `00000000-0000-4000-8000-${hex}`;
}

function isbn13(rng: () => number): string {
  let digits = "978";
  for (let i = 0; i < 10; i += 1) digits += String(Math.floor(rng() * 10));
  return digits;
}

function readingState(rng: () => number): ReadingStateSummary | null {
  if (rng() < 0.35) return null;
  const status = pick(rng, READING_STATUSES);
  return {
    status,
    rating: rng() < 0.5 ? null : 1 + Math.floor(rng() * 5),
    progress_pct: status === "reading" ? Math.round(rng() * 100) : null,
  };
}

function makeRow(index: number, rng: () => number): SpikeBookRow {
  const authorCount = 1 + Math.floor(rng() * 3);
  const authors: string[] = [];
  for (let i = 0; i < authorCount; i += 1) {
    authors.push(`${pick(rng, GIVEN)} ${pick(rng, FAMILY)}`);
  }
  return {
    id: rowId(index),
    title: `${pick(rng, TITLE_HEAD)} ${pick(rng, TITLE_TAIL)} #${String(index + 1)}`,
    subtitle: rng() < 0.4 ? pick(rng, SUBTITLES) : null,
    authors,
    isbn_13: rng() < 0.9 ? isbn13(rng) : null,
    pages: rng() < 0.95 ? 80 + Math.floor(rng() * 1200) : null,
    reading_state: readingState(rng),
  };
}

/**
 * Generate `count` deterministic rows for `seed`. Same `(seed, count)` always
 * produces the same rows in the same order.
 */
export function generateRows(seed: number, count: number = DEFAULT_ROW_COUNT): SpikeBookRow[] {
  const rng = mulberry32(seed);
  const rows: SpikeBookRow[] = new Array<SpikeBookRow>(count);
  for (let i = 0; i < count; i += 1) rows[i] = makeRow(i, rng);
  return rows;
}
