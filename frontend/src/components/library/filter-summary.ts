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

/** Props whose filter is an object keyed by operator, so a chip can drop
 *  a single operator without disturbing the rest of the state. */
type FilterOpProp = "title" | "subtitle" | "isbn13" | "pages" | "rating";

/** Build a removal patch that clears one operator on one object-filter
 *  prop. Shared by the text and range readouts, which differ only in the
 *  (runtime-erased) operator key type. */
const dropOp =
  (prop: FilterOpProp) =>
  (op: string) =>
  (current: FilterState): FilterState => ({
    ...current,
    [prop]: { ...current[prop], [op]: undefined },
  });

function pushText(
  segments: SummarySegment[],
  label: string,
  key: string,
  filter: TextFilter,
  prop: "title" | "subtitle" | "isbn13",
): void {
  const drop = dropOp(prop);
  if (filter.contains !== undefined)
    segments.push({
      key: `${key}_contains`,
      text: `${label} contains "${filter.contains}"`,
      remove: drop("contains"),
    });
  if (filter.eq !== undefined)
    segments.push({ key: `${key}_eq`, text: `${label} is "${filter.eq}"`, remove: drop("eq") });
  if (filter.ne !== undefined)
    segments.push({
      key: `${key}_ne`,
      text: `${label} is not "${filter.ne}"`,
      remove: drop("ne"),
    });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
      remove: drop("empty"),
    });
}

function pushRange(
  segments: SummarySegment[],
  label: string,
  key: string,
  filter: RangeFilter,
  prop: "pages" | "rating",
): void {
  const drop = dropOp(prop);
  if (filter.gte !== undefined)
    segments.push({
      key: `${key}_gte`,
      text: `${label} ≥ ${String(filter.gte)}`,
      remove: drop("gte"),
    });
  if (filter.lte !== undefined)
    segments.push({
      key: `${key}_lte`,
      text: `${label} ≤ ${String(filter.lte)}`,
      remove: drop("lte"),
    });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
      remove: drop("empty"),
    });
}

/**
 * None-of tokens are kept apart from the include modes: a readout that
 * renders `status_none=abandoned` as "Status: Abandoned" states the
 * opposite of the active condition, which misleads worse than saying
 * nothing at all.
 */
type SetReadout = {
  key: string;
  singular: string;
  plural: string;
  include: readonly string[];
  exclude: readonly string[];
  remove: (current: FilterState) => FilterState;
  resolve?: (token: string) => string;
};

function pushSet(segments: SummarySegment[], spec: SetReadout): void {
  const { key, singular, plural, include, exclude, remove, resolve } = spec;
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
  pushSet(segments, {
    key: "authors",
    singular: "Author",
    plural: "Authors",
    include: [...filters.authors.all, ...filters.authors.any],
    exclude: filters.authors.none,
    remove: (current) => ({ ...current, authors: emptyVocab }),
    resolve: resolvers.authorLabel,
  });
  pushSet(segments, {
    key: "tags",
    singular: "Tag",
    plural: "Tags",
    include: [...filters.tags.all, ...filters.tags.any],
    exclude: filters.tags.none,
    remove: (current) => ({ ...current, tags: emptyVocab }),
  });
  pushSet(segments, {
    key: "genres",
    singular: "Genre",
    plural: "Genres",
    include: [...filters.genres.all, ...filters.genres.any],
    exclude: filters.genres.none,
    remove: (current) => ({ ...current, genres: emptyVocab }),
  });
  pushSet(segments, {
    key: "moods",
    singular: "Mood",
    plural: "Moods",
    include: [...filters.moods.all, ...filters.moods.any],
    exclude: filters.moods.none,
    remove: (current) => ({ ...current, moods: emptyVocab }),
  });
  pushSet(segments, {
    key: "status",
    singular: "Status",
    plural: "Statuses",
    include: filters.status.any,
    exclude: filters.status.none,
    remove: (current) => ({ ...current, status: { any: [], none: [] } }),
    resolve: (token) => STATUS_LABELS[token] ?? token,
  });
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
