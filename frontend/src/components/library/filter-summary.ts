/**
 * Pure projection of the active library filter state onto the toolbar's
 * chips row: ordered, compact segments covering the full filter grammar,
 * each carrying its own removal patch. Kept apart from the chips
 * component so the projection tests need no queries.
 */
import type { FilterState, RangeFilter, TextFilter } from "@/routes/library-params";

const STATUS_LABELS: Record<string, string> = {
  unread: "Unread",
  want_to_read: "Want to read",
  reading: "Reading",
  on_hold: "On hold",
  finished: "Finished",
  abandoned: "Abandoned",
};

/** One readout segment; `key` is stable for list rendering. `remove`
 *  returns the filter state with this segment's condition dropped, so a
 *  chip can offer one-click removal without re-deriving the grammar. */
export type SummarySegment = {
  key: string;
  text: string;
  remove: (current: FilterState) => FilterState;
};

/** Render a UUID compactly (first 8 chars) for fallback labels. */
export function shortId(value: string): string {
  return value.length > 10 ? `${value.slice(0, 8)}…` : value;
}

type SummaryResolvers = {
  authorLabel: (id: string) => string;
  shelfName: (id: string) => string;
  seriesName: (id: string) => string;
};

function pushText(
  segments: SummarySegment[],
  label: string,
  key: string,
  filter: TextFilter,
  prop: "title" | "subtitle" | "isbn13",
): void {
  const dropOp = (op: keyof TextFilter) => (current: FilterState) => ({
    ...current,
    [prop]: { ...current[prop], [op]: undefined },
  });
  if (filter.contains !== undefined)
    segments.push({
      key: `${key}_contains`,
      text: `${label} contains "${filter.contains}"`,
      remove: dropOp("contains"),
    });
  if (filter.eq !== undefined)
    segments.push({ key: `${key}_eq`, text: `${label} is "${filter.eq}"`, remove: dropOp("eq") });
  if (filter.ne !== undefined)
    segments.push({
      key: `${key}_ne`,
      text: `${label} is not "${filter.ne}"`,
      remove: dropOp("ne"),
    });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
      remove: dropOp("empty"),
    });
}

function pushRange(
  segments: SummarySegment[],
  label: string,
  key: string,
  filter: RangeFilter,
  prop: "pages" | "rating",
): void {
  const dropOp = (op: keyof RangeFilter) => (current: FilterState) => ({
    ...current,
    [prop]: { ...current[prop], [op]: undefined },
  });
  if (filter.gte !== undefined)
    segments.push({
      key: `${key}_gte`,
      text: `${label} ≥ ${String(filter.gte)}`,
      remove: dropOp("gte"),
    });
  if (filter.lte !== undefined)
    segments.push({
      key: `${key}_lte`,
      text: `${label} ≤ ${String(filter.lte)}`,
      remove: dropOp("lte"),
    });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
      remove: dropOp("empty"),
    });
}

/**
 * None-of tokens are kept apart from the include modes: a readout that
 * renders `status_none=abandoned` as "Status: Abandoned" states the
 * opposite of the active condition, which misleads worse than saying
 * nothing at all.
 */
function pushSet(
  segments: SummarySegment[],
  key: string,
  singular: string,
  plural: string,
  include: readonly string[],
  exclude: readonly string[],
  remove: (current: FilterState) => FilterState,
  resolve?: (token: string) => string,
): void {
  const total = include.length + exclude.length;
  if (total === 0) return;
  if (total === 1) {
    const token = include.length === 1 ? include[0] : exclude[0];
    const name = resolve === undefined ? token : resolve(token);
    const text = include.length === 1 ? `${singular}: ${name}` : `${singular}: not ${name}`;
    segments.push({ key, text, remove });
    return;
  }
  if (exclude.length === 0) {
    segments.push({ key, text: `${plural} (${String(total)})`, remove });
    return;
  }
  if (include.length === 0) {
    segments.push({ key, text: `${plural} (${String(total)} not)`, remove });
    return;
  }
  segments.push({
    key,
    text: `${plural} (${String(include.length)}, ${String(exclude.length)} not)`,
    remove,
  });
}

/**
 * Project the active filter state onto an ordered list of compact readout
 * segments. Pure: name resolution comes in through `resolvers` so the
 * projection is testable without queries.
 */
export function buildFilterSummary(
  filters: FilterState,
  resolvers: SummaryResolvers,
): SummarySegment[] {
  const segments: SummarySegment[] = [];
  const emptyVocab = { all: [], any: [], none: [] };
  if (filters.q !== undefined)
    segments.push({
      key: "q",
      text: `Search "${filters.q}"`,
      remove: (current) => ({ ...current, q: undefined }),
    });
  if (filters.shelf !== undefined)
    segments.push({
      key: "shelf",
      text: `Shelf: ${resolvers.shelfName(filters.shelf)}`,
      remove: (current) => ({ ...current, shelf: undefined }),
    });
  if (filters.series !== undefined)
    segments.push({
      key: "series",
      text: `Series: ${resolvers.seriesName(filters.series)}`,
      remove: (current) => ({ ...current, series: undefined }),
    });
  pushSet(
    segments,
    "authors",
    "Author",
    "Authors",
    [...filters.authors.all, ...filters.authors.any],
    filters.authors.none,
    (current) => ({ ...current, authors: emptyVocab }),
    resolvers.authorLabel,
  );
  pushSet(
    segments,
    "tags",
    "Tag",
    "Tags",
    [...filters.tags.all, ...filters.tags.any],
    filters.tags.none,
    (current) => ({ ...current, tags: emptyVocab }),
  );
  pushSet(
    segments,
    "genres",
    "Genre",
    "Genres",
    [...filters.genres.all, ...filters.genres.any],
    filters.genres.none,
    (current) => ({ ...current, genres: emptyVocab }),
  );
  pushSet(
    segments,
    "moods",
    "Mood",
    "Moods",
    [...filters.moods.all, ...filters.moods.any],
    filters.moods.none,
    (current) => ({ ...current, moods: emptyVocab }),
  );
  pushSet(
    segments,
    "status",
    "Status",
    "Statuses",
    filters.status.any,
    filters.status.none,
    (current) => ({ ...current, status: { any: [], none: [] } }),
    (token) => STATUS_LABELS[token] ?? token,
  );
  pushText(segments, "Title", "title", filters.title, "title");
  pushText(segments, "Subtitle", "subtitle", filters.subtitle, "subtitle");
  pushText(segments, "ISBN", "isbn", filters.isbn13, "isbn13");
  pushRange(segments, "Pages", "pages", filters.pages, "pages");
  pushRange(segments, "Rating", "rating", filters.rating, "rating");
  if (filters.addedAfter !== undefined)
    segments.push({
      key: "added_after",
      text: `Added after ${filters.addedAfter}`,
      remove: (current) => ({ ...current, addedAfter: undefined }),
    });
  if (filters.addedBefore !== undefined)
    segments.push({
      key: "added_before",
      text: `Added before ${filters.addedBefore}`,
      remove: (current) => ({ ...current, addedBefore: undefined }),
    });
  return segments;
}
