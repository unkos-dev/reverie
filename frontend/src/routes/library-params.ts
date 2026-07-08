/**
 * URL-search ↔ filter-state codec for the `/library` route.
 *
 * Lives outside `routes/library.tsx` so the route module can keep its
 * single-component export shape (the `react/only-export-components` rule
 * requires component-only exports).
 *
 * Two views of the same URL live here:
 * - {@link paramsFromSearch} produces the flat {@link ListBooksParams} the
 *   API client sends.
 * - {@link parseFilterParams} produces the grouped {@link FilterState} the
 *   FilterBar renders and edits; {@link serializeFilterParams} writes it back.
 *
 * Parsing is tolerant by design: a malformed value (non-integer number,
 * ill-formed date, unknown status token) is dropped rather than surfaced,
 * over-long value sets are clamped, and the server remains the authority on
 * semantic validation (range, injection, existence). Nothing here filters
 * result rows; every condition round-trips to the server.
 */
import {
  parseSortParam,
  ReadingStatusSchema,
  serializeSortParam,
  type ListBooksParams,
} from "@/api";

const LIBRARY_VIEWS = ["grid", "list", "table"] as const;
/** The library's browse presentation modes. */
export type LibraryView = (typeof LIBRARY_VIEWS)[number];

/** Type guard narrowing an arbitrary string into the `LibraryView` union. */
export function isLibraryView(value: string): value is LibraryView {
  return LIBRARY_VIEWS.some((view) => view === value);
}

/**
 * Parse `?view=` into a {@link LibraryView}, or `null` when the param is
 * absent or out of range. `null` lets the caller fall back through its own
 * default chain (persisted preference, then `grid`).
 */
export function viewFromSearch(search: URLSearchParams): LibraryView | null {
  const raw = search.get("view");
  return raw !== null && isLibraryView(raw) ? raw : null;
}

/**
 * Server-side cap on the length of any multi-value filter set; the backend
 * `MAX_TAG_FILTERS` rejects a longer set with 422, so the codec clamps to keep
 * a hand-crafted URL from ever reaching that error.
 */
export const MAX_FILTER_VALUES = 20;

/**
 * Every filter URL key, in one place so `paramsFromSearch`,
 * `serializeFilterParams`, and the page's clear-all / has-active-filter checks
 * cannot drift. `q` and the single-value `series`/`shelf` filters are included;
 * pagination (`cursor`) and ordering (`sort`) are not filters and stay out.
 */
export const FILTER_PARAM_KEYS = [
  "q",
  "author",
  "author_any",
  "author_none",
  "series",
  "shelf",
  "tag",
  "tag_any",
  "tag_none",
  "genre",
  "genre_any",
  "genre_none",
  "mood",
  "mood_any",
  "mood_none",
  "status_any",
  "status_none",
  "title_contains",
  "title_eq",
  "title_ne",
  "subtitle_contains",
  "subtitle_empty",
  "isbn_13_contains",
  "isbn_13_eq",
  "isbn_13_empty",
  "pages_gte",
  "pages_lte",
  "pages_empty",
  "rating_gte",
  "rating_lte",
  "rating_empty",
  "created_at_gte",
  "created_at_lte",
] as const;

/** One reading-status token accepted by the status filter. */
const STATUS_TOKENS: ReadonlySet<string> = new Set([...ReadingStatusSchema.options, "unread"]);

/** A text-column condition. Not every column offers every operator; the editor
 *  gates which are available, the codec maps only the params that exist. */
export type TextFilter = {
  contains?: string;
  eq?: string;
  ne?: string;
  empty?: boolean;
};

/** A numeric-column condition: inclusive bounds plus an is-empty toggle. */
export type RangeFilter = {
  gte?: number;
  lte?: number;
  empty?: boolean;
};

/** A vocabulary/author condition across the three set modes. */
export type SetFilter = {
  all: string[];
  any: string[];
  none: string[];
};

/** The reading-status condition: any-of / none-of over status tokens. */
export type StatusFilter = {
  any: string[];
  none: string[];
};

/**
 * Grouped, per-column filter model the FilterBar renders. A faithful mirror of
 * the flat URL params: {@link parseFilterParams} and {@link serializeFilterParams}
 * round-trip it, and {@link filterStateToParams} projects it onto the wire shape.
 */
export type FilterState = {
  q?: string;
  title: TextFilter;
  subtitle: TextFilter;
  isbn13: TextFilter;
  pages: RangeFilter;
  rating: RangeFilter;
  addedAfter?: string;
  addedBefore?: string;
  status: StatusFilter;
  authors: SetFilter;
  tags: SetFilter;
  genres: SetFilter;
  moods: SetFilter;
  series?: string;
  shelf?: string;
};

/** A pristine {@link FilterState} with every group empty. */
export function emptyFilterState(): FilterState {
  return {
    title: {},
    subtitle: {},
    isbn13: {},
    pages: {},
    rating: {},
    status: { any: [], none: [] },
    authors: { all: [], any: [], none: [] },
    tags: { all: [], any: [], none: [] },
    genres: { all: [], any: [], none: [] },
    moods: { all: [], any: [], none: [] },
  };
}

function trimmedOrUndefined(raw: string | null): string | undefined {
  if (raw === null) return undefined;
  const trimmed = raw.trim();
  return trimmed === "" ? undefined : trimmed;
}

function boolFromSearch(raw: string | null): boolean | undefined {
  if (raw === "true") return true;
  if (raw === "false") return false;
  return undefined;
}

function intFromSearch(raw: string | null): number | undefined {
  if (raw === null || !/^-?\d+$/.test(raw)) return undefined;
  const value = Number(raw);
  // A digit string long enough to exceed the safe-integer range parses to a
  // rounded or infinite value that would serialize back as garbage (e.g.
  // "Infinity"), breaking the round-trip. Drop it like any other malformed
  // input; the server remains the authority on in-range validation.
  return Number.isSafeInteger(value) ? value : undefined;
}

function dateFromSearch(raw: string | null): string | undefined {
  return raw !== null && /^\d{4}-\d{2}-\d{2}$/.test(raw) ? raw : undefined;
}

/** De-duplicate, drop empties, and clamp a multi-value set to the server cap. */
function setFromSearch(search: URLSearchParams, key: string): string[] {
  const values = search.getAll(key).filter((value) => value !== "");
  return [...new Set(values)].slice(0, MAX_FILTER_VALUES);
}

/** Status tokens, filtered to the known set (unknown tokens the server would
 *  422 are dropped rather than carried). */
function statusFromSearch(search: URLSearchParams, key: string): string[] {
  return setFromSearch(search, key).filter((token) => STATUS_TOKENS.has(token));
}

function textFromSearch(search: URLSearchParams, prefix: string): TextFilter {
  const filter: TextFilter = {};
  const contains = trimmedOrUndefined(search.get(`${prefix}_contains`));
  if (contains !== undefined) filter.contains = contains;
  const eq = trimmedOrUndefined(search.get(`${prefix}_eq`));
  if (eq !== undefined) filter.eq = eq;
  const ne = trimmedOrUndefined(search.get(`${prefix}_ne`));
  if (ne !== undefined) filter.ne = ne;
  const empty = boolFromSearch(search.get(`${prefix}_empty`));
  if (empty !== undefined) filter.empty = empty;
  return filter;
}

function rangeFromSearch(search: URLSearchParams, prefix: string): RangeFilter {
  const filter: RangeFilter = {};
  const gte = intFromSearch(search.get(`${prefix}_gte`));
  if (gte !== undefined) filter.gte = gte;
  const lte = intFromSearch(search.get(`${prefix}_lte`));
  if (lte !== undefined) filter.lte = lte;
  const empty = boolFromSearch(search.get(`${prefix}_empty`));
  if (empty !== undefined) filter.empty = empty;
  return filter;
}

/**
 * Parse the URL search params into a grouped {@link FilterState}. Tolerant:
 * malformed values are dropped, sets are de-duplicated and clamped, and unknown
 * status tokens are discarded. Absent keys leave their group empty.
 */
export function parseFilterParams(search: URLSearchParams): FilterState {
  const state = emptyFilterState();
  state.q = trimmedOrUndefined(search.get("q"));
  state.title = textFromSearch(search, "title");
  state.subtitle = textFromSearch(search, "subtitle");
  state.isbn13 = textFromSearch(search, "isbn_13");
  state.pages = rangeFromSearch(search, "pages");
  state.rating = rangeFromSearch(search, "rating");
  state.addedAfter = dateFromSearch(search.get("created_at_gte"));
  state.addedBefore = dateFromSearch(search.get("created_at_lte"));
  state.status = {
    any: statusFromSearch(search, "status_any"),
    none: statusFromSearch(search, "status_none"),
  };
  state.authors = {
    all: setFromSearch(search, "author"),
    any: setFromSearch(search, "author_any"),
    none: setFromSearch(search, "author_none"),
  };
  state.tags = {
    all: setFromSearch(search, "tag"),
    any: setFromSearch(search, "tag_any"),
    none: setFromSearch(search, "tag_none"),
  };
  state.genres = {
    all: setFromSearch(search, "genre"),
    any: setFromSearch(search, "genre_any"),
    none: setFromSearch(search, "genre_none"),
  };
  state.moods = {
    all: setFromSearch(search, "mood"),
    any: setFromSearch(search, "mood_any"),
    none: setFromSearch(search, "mood_none"),
  };
  state.series = trimmedOrUndefined(search.get("series"));
  state.shelf = trimmedOrUndefined(search.get("shelf"));
  return state;
}

function setText(search: URLSearchParams, key: string, value: string | undefined): void {
  if (value !== undefined && value !== "") search.set(key, value);
}

function setBool(search: URLSearchParams, key: string, value: boolean | undefined): void {
  if (value !== undefined) search.set(key, String(value));
}

function setNumber(search: URLSearchParams, key: string, value: number | undefined): void {
  if (value !== undefined) search.set(key, String(value));
}

function appendSet(search: URLSearchParams, key: string, values: readonly string[]): void {
  for (const value of values.slice(0, MAX_FILTER_VALUES)) {
    if (value !== "") search.append(key, value);
  }
}

function serializeText(search: URLSearchParams, prefix: string, filter: TextFilter): void {
  setText(search, `${prefix}_contains`, filter.contains);
  setText(search, `${prefix}_eq`, filter.eq);
  setText(search, `${prefix}_ne`, filter.ne);
  setBool(search, `${prefix}_empty`, filter.empty);
}

function serializeRange(search: URLSearchParams, prefix: string, filter: RangeFilter): void {
  setNumber(search, `${prefix}_gte`, filter.gte);
  setNumber(search, `${prefix}_lte`, filter.lte);
  setBool(search, `${prefix}_empty`, filter.empty);
}

/**
 * Write a {@link FilterState} onto `search` in place, delete-then-set: every
 * filter key is cleared first so a dropped condition leaves no stale param,
 * then each active condition is written. `cursor` is the caller's concern (a
 * filter change invalidates the keyset boundary); this touches filter keys only.
 */
export function serializeFilterParams(state: FilterState, search: URLSearchParams): void {
  for (const key of FILTER_PARAM_KEYS) search.delete(key);
  setText(search, "q", state.q);
  serializeText(search, "title", state.title);
  serializeText(search, "subtitle", state.subtitle);
  serializeText(search, "isbn_13", state.isbn13);
  serializeRange(search, "pages", state.pages);
  serializeRange(search, "rating", state.rating);
  setText(search, "created_at_gte", state.addedAfter);
  setText(search, "created_at_lte", state.addedBefore);
  appendSet(search, "status_any", state.status.any);
  appendSet(search, "status_none", state.status.none);
  appendSet(search, "author", state.authors.all);
  appendSet(search, "author_any", state.authors.any);
  appendSet(search, "author_none", state.authors.none);
  appendSet(search, "tag", state.tags.all);
  appendSet(search, "tag_any", state.tags.any);
  appendSet(search, "tag_none", state.tags.none);
  appendSet(search, "genre", state.genres.all);
  appendSet(search, "genre_any", state.genres.any);
  appendSet(search, "genre_none", state.genres.none);
  appendSet(search, "mood", state.moods.all);
  appendSet(search, "mood_any", state.moods.any);
  appendSet(search, "mood_none", state.moods.none);
  setText(search, "series", state.series);
  setText(search, "shelf", state.shelf);
}

function assignText(
  params: ListBooksParams,
  prefix: "title" | "subtitle" | "isbn_13",
  filter: TextFilter,
): void {
  if (filter.contains !== undefined) params[`${prefix}_contains`] = filter.contains;
  if (prefix === "title" && filter.eq !== undefined) params.title_eq = filter.eq;
  if (prefix === "title" && filter.ne !== undefined) params.title_ne = filter.ne;
  if (prefix === "isbn_13" && filter.eq !== undefined) params.isbn_13_eq = filter.eq;
  if ((prefix === "subtitle" || prefix === "isbn_13") && filter.empty !== undefined) {
    params[`${prefix}_empty`] = filter.empty;
  }
}

/** The `ListBooksParams` keys whose value is a string array (the vocab/author
 *  set filters), so `assignSet` can index them without widening to `never`. */
type ArrayParamKey = {
  [K in keyof ListBooksParams]-?: NonNullable<ListBooksParams[K]> extends readonly string[]
    ? K
    : never;
}[keyof ListBooksParams];

function assignSet(
  params: ListBooksParams,
  keys: readonly [ArrayParamKey, ArrayParamKey, ArrayParamKey],
  filter: SetFilter,
): void {
  const [all, any, none] = keys;
  if (filter.all.length > 0) params[all] = filter.all;
  if (filter.any.length > 0) params[any] = filter.any;
  if (filter.none.length > 0) params[none] = filter.none;
}

/**
 * Project a {@link FilterState} onto the flat {@link ListBooksParams} the client
 * sends. Empty groups contribute nothing, so the URL omits inactive filters.
 */
export function filterStateToParams(state: FilterState): ListBooksParams {
  const params: ListBooksParams = {};
  // Guard blanks, not just `undefined`: the text codec treats "" as "no value",
  // so an empty `q`/`series`/`shelf` must not project onto the wire (nor count
  // as an active filter via `hasActiveFilterState`).
  if (state.q !== undefined && state.q !== "") params.q = state.q;
  assignText(params, "title", state.title);
  assignText(params, "subtitle", state.subtitle);
  assignText(params, "isbn_13", state.isbn13);
  if (state.pages.gte !== undefined) params.pages_gte = state.pages.gte;
  if (state.pages.lte !== undefined) params.pages_lte = state.pages.lte;
  if (state.pages.empty !== undefined) params.pages_empty = state.pages.empty;
  if (state.rating.gte !== undefined) params.rating_gte = state.rating.gte;
  if (state.rating.lte !== undefined) params.rating_lte = state.rating.lte;
  if (state.rating.empty !== undefined) params.rating_empty = state.rating.empty;
  if (state.addedAfter !== undefined) params.created_at_gte = state.addedAfter;
  if (state.addedBefore !== undefined) params.created_at_lte = state.addedBefore;
  if (state.status.any.length > 0) params.status_any = state.status.any;
  if (state.status.none.length > 0) params.status_none = state.status.none;
  assignSet(params, ["author", "author_any", "author_none"], state.authors);
  assignSet(params, ["tag", "tag_any", "tag_none"], state.tags);
  assignSet(params, ["genre", "genre_any", "genre_none"], state.genres);
  assignSet(params, ["mood", "mood_any", "mood_none"], state.moods);
  if (state.series !== undefined && state.series !== "") params.series = state.series;
  if (state.shelf !== undefined && state.shelf !== "") params.shelf = state.shelf;
  return params;
}

/** True when any filter condition is active in `state`. */
export function hasActiveFilterState(state: FilterState): boolean {
  return Object.keys(filterStateToParams(state)).length > 0;
}

/**
 * Parse the URL search params into a {@link ListBooksParams}. `sort` is
 * round-tripped through `parseSortParam`/`serializeSortParam` so invalid levels
 * (unknown fields, duplicates, a stack past the cap) are normalized out; an
 * absent or all-invalid `sort` omits the key so the backend applies its default
 * order. Filter params flow through the tolerant {@link parseFilterParams} codec.
 */
export function paramsFromSearch(search: URLSearchParams): ListBooksParams {
  const params = filterStateToParams(parseFilterParams(search));
  const sort = serializeSortParam(parseSortParam(search.get("sort") ?? ""));
  if (sort !== "") params.sort = sort;
  const cursor = search.get("cursor");
  if (cursor !== null) params.cursor = cursor;
  return params;
}
