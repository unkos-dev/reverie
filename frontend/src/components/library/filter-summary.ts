/**
 * Pure projection of the active library filter state onto the masthead
 * readout: ordered, compact segments covering the full filter grammar.
 * Kept apart from the `FilterSummary` component so the component file
 * exports components only, and the projection tests need no queries.
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

/** One readout segment; `key` is stable for list rendering. */
export type SummarySegment = { key: string; text: string };

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
): void {
  if (filter.contains !== undefined)
    segments.push({ key: `${key}_contains`, text: `${label} contains "${filter.contains}"` });
  if (filter.eq !== undefined)
    segments.push({ key: `${key}_eq`, text: `${label} is "${filter.eq}"` });
  if (filter.ne !== undefined)
    segments.push({ key: `${key}_ne`, text: `${label} is not "${filter.ne}"` });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
    });
}

function pushRange(
  segments: SummarySegment[],
  label: string,
  key: string,
  filter: RangeFilter,
): void {
  if (filter.gte !== undefined)
    segments.push({ key: `${key}_gte`, text: `${label} ≥ ${String(filter.gte)}` });
  if (filter.lte !== undefined)
    segments.push({ key: `${key}_lte`, text: `${label} ≤ ${String(filter.lte)}` });
  if (filter.empty !== undefined)
    segments.push({
      key: `${key}_empty`,
      text: filter.empty ? `${label} is empty` : `${label} is set`,
    });
}

function pushSet(
  segments: SummarySegment[],
  key: string,
  singular: string,
  plural: string,
  tokens: readonly string[],
  resolve?: (token: string) => string,
): void {
  if (tokens.length === 0) return;
  if (tokens.length === 1) {
    const name = resolve === undefined ? tokens[0] : resolve(tokens[0]);
    segments.push({ key, text: `${singular}: ${name}` });
    return;
  }
  segments.push({ key, text: `${plural} (${String(tokens.length)})` });
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
  if (filters.q !== undefined) segments.push({ key: "q", text: `Search "${filters.q}"` });
  if (filters.shelf !== undefined)
    segments.push({ key: "shelf", text: `Shelf: ${resolvers.shelfName(filters.shelf)}` });
  if (filters.series !== undefined)
    segments.push({ key: "series", text: `Series: ${resolvers.seriesName(filters.series)}` });
  const authorTokens = [...filters.authors.all, ...filters.authors.any, ...filters.authors.none];
  pushSet(segments, "authors", "Author", "Authors", authorTokens, resolvers.authorLabel);
  pushSet(segments, "tags", "Tag", "Tags", [
    ...filters.tags.all,
    ...filters.tags.any,
    ...filters.tags.none,
  ]);
  pushSet(segments, "genres", "Genre", "Genres", [
    ...filters.genres.all,
    ...filters.genres.any,
    ...filters.genres.none,
  ]);
  pushSet(segments, "moods", "Mood", "Moods", [
    ...filters.moods.all,
    ...filters.moods.any,
    ...filters.moods.none,
  ]);
  pushSet(
    segments,
    "status",
    "Status",
    "Statuses",
    [...filters.status.any, ...filters.status.none],
    (token) => STATUS_LABELS[token] ?? token,
  );
  pushText(segments, "Title", "title", filters.title);
  pushText(segments, "Subtitle", "subtitle", filters.subtitle);
  pushText(segments, "ISBN", "isbn", filters.isbn13);
  pushRange(segments, "Pages", "pages", filters.pages);
  pushRange(segments, "Rating", "rating", filters.rating);
  if (filters.addedAfter !== undefined)
    segments.push({ key: "added_after", text: `Added after ${filters.addedAfter}` });
  if (filters.addedBefore !== undefined)
    segments.push({ key: "added_before", text: `Added before ${filters.addedBefore}` });
  return segments;
}
