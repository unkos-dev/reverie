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
 *   filter rail renders and edits; {@link serializeFilterParams} writes it back.
 *
 * Parsing is tolerant by design: a malformed value (non-integer number,
 * ill-formed date, unknown status token) is dropped rather than surfaced,
 * over-long value sets are clamped, and the server remains the authority on
 * semantic validation (range, injection, existence). Nothing here filters
 * result rows; every condition round-trips to the server.
 */
import {
  isLibraryView,
  ReadingStatusSchema,
  type ArrayParamKey,
  type LibraryView,
  type ListBooksParams,
} from "@/api";

// The view vocabulary is a Postgres enum on the preferences resource, so it
// is declared once at the API boundary and re-exported here for the `?view=`
// codec's existing consumers.
export { isLibraryView };
export type { LibraryView };

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

/** The full text-operator vocabulary; each column supports the subset in
 *  {@link TEXT_COLUMN_OPS}. `empty` filters on the column being null. */
export const TEXT_OPS = ["contains", "eq", "ne", "empty"] as const;
export type TextOp = (typeof TEXT_OPS)[number];

/** Type guard narrowing an arbitrary string into the `TextOp` union. */
export function isTextOp(value: string): value is TextOp {
  return TEXT_OPS.some((op) => op === value);
}

const TEXT_COLUMNS = ["title", "subtitle", "isbn_13"] as const;
type TextColumn = (typeof TEXT_COLUMNS)[number];

/** The subset of `TextOp` backed by a wire field for column `C`: exactly the
 *  ops whose `${C}_${op}` key exists on {@link ListBooksParams}, so declaring
 *  an op the API cannot serve fails compilation. */
type WireOp<C extends TextColumn> = {
  [O in TextOp]: `${C}_${O}` extends keyof ListBooksParams ? O : never;
}[TextOp];

/**
 * The one declaration of which operators each text column offers. The codec
 * parses and serializes only these, {@link filterStateToParams} sends only
 * these, and the filter rail builds each column's operator select from this
 * table, so an op missing here cannot become URL state, a chip, or a wire
 * param anywhere. `title` has no `empty` (the backend column is `NOT NULL`,
 * so the condition could never match a row); `subtitle` offers containment
 * and emptiness only; `isbn_13` has no `ne`. Order is the operator-select
 * order in the rail.
 */
export const TEXT_COLUMN_OPS: { readonly [C in TextColumn]: readonly WireOp<C>[] } = {
  title: ["contains", "eq", "ne"],
  subtitle: ["contains", "empty"],
  isbn_13: ["contains", "eq", "empty"],
};

/** The text-condition URL keys with live predicates, derived from
 *  {@link TEXT_COLUMN_OPS} so the filter-key list cannot drift from it. */
const TEXT_FILTER_PARAM_KEYS: readonly string[] = TEXT_COLUMNS.flatMap((column) =>
  TEXT_COLUMN_OPS[column].map((op) => `${column}_${op}`),
);

/**
 * Text params with no wire predicate: the column/op pairs absent from
 * {@link TEXT_COLUMN_OPS} (e.g. `title_empty`, `subtitle_eq`). Nothing parses
 * or sends them, but a hand-crafted or stale URL can still carry one, so
 * `serializeFilterParams`'s delete-then-set sweep and the page's clear-all
 * purge them. They are deliberately not in {@link FILTER_PARAM_KEYS}: a param
 * that is never sent must not count as an active filter (toolbar badge,
 * filtered-empty-state selection).
 */
export const PURGE_ONLY_PARAM_KEYS: readonly string[] = TEXT_COLUMNS.flatMap((column) => {
  const supported: readonly TextOp[] = TEXT_COLUMN_OPS[column];
  return TEXT_OPS.filter((op) => !supported.includes(op)).map((op) => `${column}_${op}`);
});

/**
 * Every live filter URL key, in one place so `serializeFilterParams`'s
 * delete-then-set sweep and the page's clear-all cannot drift from the codec.
 * `q` and the single-value `series`/`shelf` filters are included; pagination
 * (`cursor`) and ordering (`sort`) are not filters and stay out. Dead params
 * a URL might still carry live in {@link PURGE_ONLY_PARAM_KEYS}, which shares
 * the purge sweeps. Active-filter checks (badge count, empty-state split) do
 * not read either list: they derive from {@link filterStateToParams}, so only
 * a condition that actually reaches the wire counts as active.
 */
export const FILTER_PARAM_KEYS: readonly string[] = [
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
  ...TEXT_FILTER_PARAM_KEYS,
  "pages_gte",
  "pages_lte",
  "pages_empty",
  "rating_gte",
  "rating_lte",
  "rating_empty",
  "created_at_gte",
  "created_at_lte",
];

/** One reading-status token accepted by the status filter. */
const STATUS_TOKENS: ReadonlySet<string> = new Set([...ReadingStatusSchema.options, "unread"]);

/** A text-column condition. Not every column offers every operator;
 *  {@link TEXT_COLUMN_OPS} declares each column's set. */
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
 * Grouped, per-column filter model the filter rail renders. A faithful mirror of
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

/**
 * Parse a text-column condition, reading only the ops {@link TEXT_COLUMN_OPS}
 * declares for the column. A param outside that set (`?title_empty=`,
 * `?subtitle_eq=`) has no wire predicate, so it must be dropped here rather
 * than surfacing as a chip for a condition that is silently never sent.
 */
function textFromSearch(search: URLSearchParams, column: TextColumn): TextFilter {
  const filter: TextFilter = {};
  for (const op of TEXT_COLUMN_OPS[column]) {
    if (op === "empty") {
      const empty = boolFromSearch(search.get(`${column}_empty`));
      if (empty !== undefined) filter.empty = empty;
    } else {
      const value = trimmedOrUndefined(search.get(`${column}_${op}`));
      if (value !== undefined) filter[op] = value;
    }
  }
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

function serializeText(search: URLSearchParams, column: TextColumn, filter: TextFilter): void {
  for (const op of TEXT_COLUMN_OPS[column]) {
    if (op === "empty") setBool(search, `${column}_empty`, filter.empty);
    else setText(search, `${column}_${op}`, filter[op]);
  }
}

function serializeRange(search: URLSearchParams, prefix: string, filter: RangeFilter): void {
  setNumber(search, `${prefix}_gte`, filter.gte);
  setNumber(search, `${prefix}_lte`, filter.lte);
  setBool(search, `${prefix}_empty`, filter.empty);
}

/**
 * Write a {@link FilterState} onto `search` in place, delete-then-set: every
 * filter key (dead ones included) is cleared first so a dropped condition or
 * a stale ghost param leaves nothing behind, then each active condition is
 * written. `cursor` is the caller's concern (a filter change invalidates the
 * keyset boundary); this touches filter keys only.
 */
export function serializeFilterParams(state: FilterState, search: URLSearchParams): void {
  for (const key of FILTER_PARAM_KEYS) search.delete(key);
  for (const key of PURGE_ONLY_PARAM_KEYS) search.delete(key);
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

/**
 * Project one text condition onto the wire params. The conditionals mirror
 * {@link TEXT_COLUMN_OPS} in the explicit form the compiler can check against
 * `ListBooksParams` (a dynamic `${column}_${op}` key cannot be proven to
 * exist without a cast); the codec tests walk the ops table end to end to
 * keep this projection from drifting out of it.
 */
function assignText(params: ListBooksParams, prefix: TextColumn, filter: TextFilter): void {
  if (filter.contains !== undefined) params[`${prefix}_contains`] = filter.contains;
  if (prefix === "title" && filter.eq !== undefined) params.title_eq = filter.eq;
  if (prefix === "title" && filter.ne !== undefined) params.title_ne = filter.ne;
  if (prefix === "isbn_13" && filter.eq !== undefined) params.isbn_13_eq = filter.eq;
  if ((prefix === "subtitle" || prefix === "isbn_13") && filter.empty !== undefined) {
    params[`${prefix}_empty`] = filter.empty;
  }
}

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

/** Project a plain string field onto `params`, treating "" the same as absent.
 *  The text codec reads "" as "no value", so a blank `q`/`series`/`shelf` must
 *  not reach the wire (nor count as active via `hasActiveFilterState`). */
function setIfNonEmpty(
  params: ListBooksParams,
  key: "q" | "series" | "shelf",
  value: string | undefined,
): void {
  if (value !== undefined && value !== "") params[key] = value;
}

/**
 * Project a {@link FilterState} onto the flat {@link ListBooksParams} the client
 * sends. Empty groups contribute nothing, so the URL omits inactive filters.
 */
export function filterStateToParams(state: FilterState): ListBooksParams {
  const params: ListBooksParams = {};
  setIfNonEmpty(params, "q", state.q);
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
  setIfNonEmpty(params, "series", state.series);
  setIfNonEmpty(params, "shelf", state.shelf);
  return params;
}

/** True when any filter condition is active in `state`. */
export function hasActiveFilterState(state: FilterState): boolean {
  return Object.keys(filterStateToParams(state)).length > 0;
}

/**
 * Parse the URL search params into a {@link ListBooksParams}. Filter params
 * flow through the tolerant {@link parseFilterParams} codec. `sort` is
 * deliberately not read: the library's sort is a per-user preference with no
 * URL form (`docs/adr/0047-library-sort-is-a-per-user-preference-never-url-state.md`), so a
 * stale `?sort=` from an old bookmark is inert and the caller supplies any
 * sort override from the preference surface instead.
 */
export function paramsFromSearch(search: URLSearchParams): ListBooksParams {
  const params = filterStateToParams(parseFilterParams(search));
  const cursor = search.get("cursor");
  if (cursor !== null) params.cursor = cursor;
  return params;
}
