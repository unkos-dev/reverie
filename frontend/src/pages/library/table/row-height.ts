/**
 * Row heights for the Library table. Title and Authors wrap, and
 * react-data-grid has no content-based height, so each row's height comes from
 * an estimate of its wrapped line count: greedy word wrap over canvas
 * `measureText` widths in the grid's own font. Every wrapped line is 20px
 * (`leading-5`), plus 12px above and below.
 */
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
 * where a word wider than the line takes `ceil(wordWidth / width)` lines.
 * Empty text is one line. `cacheKey` distinguishes the same font string before
 * and after the web font loads.
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
      const spanned = Math.ceil(w / width);
      lines += spanned - 1;
      lineWidth = w - (spanned - 1) * width;
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
