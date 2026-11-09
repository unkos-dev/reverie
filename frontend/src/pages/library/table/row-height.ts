/**
 * Row heights for the Library table. Title and Authors wrap, and
 * react-data-grid takes each row's height from the app, so rendered rows use
 * the measured height of their wrapped text plus 12px above and below. Rows
 * not yet rendered use an estimate of their wrapped line count: greedy word
 * wrap over canvas `measureText` widths in the grid's own font, 20px a line
 * (`leading-5`).
 */
import { useEffect, useState } from "react";

import type { BookListItem, Density } from "@/api";

/** Row heights per density with no wrapped lines; the header stays fixed across both. */
export const ROW_HEIGHT_BY_DENSITY: Record<Density, number> = { comfortable: 64, compact: 44 };

const LINE_HEIGHT = 20;
const ROW_PADDING = 24;
/** An RDG cell's 8px inline padding each side plus its 1px inline-end border. */
const CELL_INSET = 17;
/** The comfortable Title cell's 30px cover mark and its 12px gap. */
const COVER_AND_GAP = 42;
const EMPTY_CELL = "—";

export type RowHeightLayout = {
  density: Density;
  isMobile: boolean;
  /** Title column width; `undefined` until the grid has measured it. */
  titleWidth: number | undefined;
  /** Authors column width; `null` while the column is hidden, `undefined` until measured. */
  authorsWidth: number | null | undefined;
  /** The grid's computed `font-family`. */
  fontFamily: string;
  /** Whether web fonts have loaded. `font-display: swap` keeps the family string unchanged across the swap. */
  fontsLoaded: boolean;
};

let context: CanvasRenderingContext2D | null | undefined;
const wordWidthsByFont = new Map<string, Map<string, number>>();

function measureContext(): CanvasRenderingContext2D | null {
  if (context === undefined) {
    context = document.createElement("canvas").getContext("2d");
    if (context === null)
      console.error("[row-height] no 2D canvas context; rows keep their base height");
  }
  return context;
}

function wordWidth(
  ctx: CanvasRenderingContext2D,
  font: string,
  cacheKey: string,
  word: string,
): number {
  let widths = wordWidthsByFont.get(cacheKey);
  if (widths === undefined) {
    widths = new Map();
    wordWidthsByFont.set(cacheKey, widths);
  }
  let width = widths.get(word);
  if (width === undefined) {
    ctx.font = font;
    width = ctx.measureText(word).width;
    widths.set(word, width);
  }
  return width;
}

/**
 * Wrapped line count of `text` in `font` at `width` pixels: greedy word wrap,
 * where a word wider than the line fills a line of its own, clipped. Empty
 * text is one line. `cacheKey` distinguishes the same font string before and
 * after the web font loads.
 */
export function estimateLines(text: string, font: string, width: number, cacheKey = font): number {
  const words = text.split(/\s+/).filter((word) => word !== "");
  const ctx = measureContext();
  if (words.length === 0 || width <= 0 || ctx === null) return 1;
  const space = wordWidth(ctx, font, cacheKey, " ");
  let lines = 1;
  let lineWidth = 0;
  for (const word of words) {
    const w = wordWidth(ctx, font, cacheKey, word);
    if (w > width) {
      if (lineWidth > 0) lines += 1;
      lineWidth = width;
    } else if (lineWidth === 0) {
      lineWidth = w;
    } else if (lineWidth + space + w <= width) {
      lineWidth += space + w;
    } else {
      lines += 1;
      lineWidth = w;
    }
  }
  return lines;
}

type CachedLines = {
  titleTextWidth: number;
  authorsTextWidth: number | null;
  isMobile: boolean;
  fontKey: string;
  lines: number;
};

const linesByRow = new WeakMap<BookListItem, CachedLines>();

function authorsText(row: BookListItem): string {
  return row.authors.length > 0 ? row.authors.join(", ") : EMPTY_CELL;
}

/**
 * Height of one row: the density base, or `lines × 20 + 24` when the larger of
 * the Title and Authors line counts needs more. Line counts are cached per row
 * object, so a re-run over every row recomputes only new or changed rows, or
 * all of them after a width, font or layout change.
 */
export function rowHeightFor(row: BookListItem, layout: RowHeightLayout): number {
  const base = ROW_HEIGHT_BY_DENSITY[layout.density];
  const { titleWidth, authorsWidth } = layout;
  if (titleWidth === undefined || authorsWidth === undefined) return base;

  const showsCover = layout.density === "comfortable";
  const titleTextWidth = titleWidth - CELL_INSET - (showsCover ? COVER_AND_GAP : 0);
  const authorsTextWidth = authorsWidth === null ? null : authorsWidth - CELL_INSET;
  const loaded = String(layout.fontsLoaded);
  const fontKey = `${layout.fontFamily}|${loaded}`;

  const cached = linesByRow.get(row);
  if (
    cached?.titleTextWidth === titleTextWidth &&
    cached.authorsTextWidth === authorsTextWidth &&
    cached.isMobile === layout.isMobile &&
    cached.fontKey === fontKey
  ) {
    return Math.max(base, cached.lines * LINE_HEIGHT + ROW_PADDING);
  }

  const titleFont = `500 14px ${layout.fontFamily}`;
  const bodyFont = `400 14px ${layout.fontFamily}`;
  let titleLines = estimateLines(row.title, titleFont, titleTextWidth, `${titleFont}|${loaded}`);
  if (layout.isMobile && showsCover) {
    const authorLineFont = `400 12px ${layout.fontFamily}`;
    titleLines += estimateLines(
      authorsText(row),
      authorLineFont,
      titleTextWidth,
      `${authorLineFont}|${loaded}`,
    );
  }
  const authorsLines =
    authorsTextWidth === null
      ? 0
      : estimateLines(authorsText(row), bodyFont, authorsTextWidth, `${bodyFont}|${loaded}`);
  const lines = Math.max(titleLines, authorsLines);
  linesByRow.set(row, {
    titleTextWidth,
    authorsTextWidth,
    isMobile: layout.isMobile,
    fontKey,
    lines,
  });
  return Math.max(base, lines * LINE_HEIGHT + ROW_PADDING);
}

/** A measured text element: the Title link's text, the mobile author line under it, or the Authors text. */
export type MeasuredPart = "title" | "authorLine" | "authors";

type MeasuredRow = Partial<Record<MeasuredPart, number>>;

/** What a measured row needs from the grid's current layout. */
export type MeasureLayout = {
  /** Changes with any column width, density or layout change that invalidates measurements. */
  key: string;
  density: Density;
  expectsAuthorLine: boolean;
  expectsAuthors: boolean;
};

/** Returns a row's measured heights, or `undefined` when none were taken under `layoutKey`. */
export type MeasuredReader = (rowId: string, layoutKey: string) => MeasuredRow | undefined;

/** Ref callback for one measured text element of a row. */
export type MeasureRef = (
  rowId: string,
  part: MeasuredPart,
) => (element: HTMLElement | null) => (() => void) | undefined;

/** Re-keying `rowHeight` recomputes every loaded row, so only a visible difference starts one. */
const SHOWN_HEIGHT_TOLERANCE = 2;

function measuredRowHeight(parts: MeasuredRow, density: Density): number {
  const title = (parts.title ?? 0) + (parts.authorLine ?? 0);
  const authors = parts.authors ?? 0;
  return Math.max(ROW_HEIGHT_BY_DENSITY[density], Math.max(title, authors) + ROW_PADDING);
}

function isMeasured(parts: MeasuredRow, layout: MeasureLayout): boolean {
  return (
    parts.title !== undefined &&
    (!layout.expectsAuthorLine || parts.authorLine !== undefined) &&
    (!layout.expectsAuthors || parts.authors !== undefined)
  );
}

/**
 * A row's height: its measured text when every part has been measured,
 * otherwise the larger of the estimate and whatever has been measured.
 */
export function resolveRowHeight(
  estimate: number,
  parts: MeasuredRow | undefined,
  layout: MeasureLayout,
): number {
  if (parts === undefined) return estimate;
  const measured = measuredRowHeight(parts, layout.density);
  return isMeasured(parts, layout) ? measured : Math.max(estimate, measured);
}

class RowMeasurer {
  #layout: MeasureLayout | null = null;
  #rowsKey: string | null = null;
  #rows = new Map<string, MeasuredRow>();
  #targets = new Map<Element, { rowId: string; part: MeasuredPart }>();
  #observer: ResizeObserver | null = null;
  #frame = 0;

  readonly measure: MeasureRef = (rowId, part) => (element) => {
    if (element === null) return undefined;
    this.#targets.set(element, { rowId, part });
    this.#observer?.observe(element);
    return () => {
      this.#targets.delete(element);
      this.#observer?.unobserve(element);
    };
  };

  reader(): MeasuredReader {
    return (rowId, layoutKey) => (this.#rowsKey === layoutKey ? this.#rows.get(rowId) : undefined);
  }

  /** Starts observing; `onChange` runs at most once a frame, when a rendered row's shown height is off. */
  connect(onChange: () => void): () => void {
    const observer = new ResizeObserver((entries) => {
      if (this.#record(entries) && this.#frame === 0) {
        this.#frame = requestAnimationFrame(() => {
          this.#frame = 0;
          onChange();
        });
      }
    });
    this.#observer = observer;
    for (const element of this.#targets.keys()) observer.observe(element);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(this.#frame);
      this.#frame = 0;
      this.#observer = null;
    };
  }

  /** Measurements taken under another layout no longer apply; rendered rows are measured again. */
  readonly applyLayout = (layout: MeasureLayout): void => {
    const changed = layout.key !== this.#layout?.key;
    this.#layout = layout;
    if (!changed || this.#observer === null) return;
    for (const element of this.#targets.keys()) {
      this.#observer.unobserve(element);
      this.#observer.observe(element);
    }
  };

  #record(entries: readonly ResizeObserverEntry[]): boolean {
    const layout = this.#layout;
    if (layout === null) return false;
    if (this.#rowsKey !== layout.key) {
      this.#rows = new Map();
      this.#rowsKey = layout.key;
    }
    const touched = new Map<HTMLElement, string>();
    for (const entry of entries) {
      const target = this.#targets.get(entry.target);
      if (target === undefined) continue;
      const parts = this.#rows.get(target.rowId);
      this.#rows.set(target.rowId, { ...parts, [target.part]: entry.contentRect.height });
      const row = entry.target.closest('[role="row"]');
      if (row instanceof HTMLElement) touched.set(row, target.rowId);
    }
    for (const [row, rowId] of touched) {
      const parts = this.#rows.get(rowId);
      if (parts === undefined) continue;
      const measured = measuredRowHeight(parts, layout.density);
      const shown = row.offsetHeight;
      const off = isMeasured(parts, layout)
        ? Math.abs(measured - shown) > SHOWN_HEIGHT_TOLERANCE
        : measured > shown + SHOWN_HEIGHT_TOLERANCE;
      if (off) return true;
    }
    return false;
  }
}

/**
 * Measures the rendered Title and Authors text of each row with one
 * `ResizeObserver`. `read` changes identity whenever a rendered row's shown
 * height no longer matches its text, so a `rowHeight` built on it re-keys.
 * Measurements outlive their row's unmount and are dropped when the key passed
 * to `applyLayout` changes; nothing is recorded before the first call.
 */
export function useRowMeasurement(): {
  measure: MeasureRef;
  read: MeasuredReader;
  applyLayout: (layout: MeasureLayout) => void;
} {
  const [measurer] = useState(() => new RowMeasurer());
  const [read, setRead] = useState(() => measurer.reader());
  useEffect(
    () =>
      measurer.connect(() => {
        setRead(() => measurer.reader());
      }),
    [measurer],
  );
  return { measure: measurer.measure, read, applyLayout: measurer.applyLayout };
}
