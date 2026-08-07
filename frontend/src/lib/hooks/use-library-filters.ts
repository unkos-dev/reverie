/**
 * Typed per-key URL state for the library's filter grammar.
 *
 * The URL is the state container for library filters because of the
 * lifetime it gives them: they must survive a refresh and in-app
 * navigation, but must not survive into a fresh visit days later, and no
 * other medium has that shape without extra machinery.
 *
 * Each editing surface owns the keys it edits and reads nothing else,
 * which is what makes cross-surface coordination unnecessary rather than
 * merely cheaper. Two properties of the underlying queue carry it:
 *
 * - A write to a key cancels that key's own pending debounced write
 *   before queueing the new one, because a write that does not ask for
 *   `limitUrlUpdates: debounce(...)` aborts the key's debounce queue
 *   first. A clear therefore cannot be overtaken by the draft it was
 *   meant to discard, and needs no fire-time staleness check to prove it.
 * - Writes to different keys in the same tick merge into one URL update
 *   applied to a single snapshot, so two writers in one frame cannot
 *   build on stale copies and clobber each other.
 *
 * The corollary is a rule, not a style preference: a gesture must write
 * only the keys it actually edits. Writing the whole grammar on every
 * commit would push a sibling slice's current value into that slice's
 * debounce queue and overwrite a draft the user is still typing.
 *
 * Reads that need the whole grammar at once (the wire params, the active
 * count, the chips) go through {@link useLibrarySearchParams} and the
 * codec in `routes/library-params`, which remains the one parser and
 * validator for every filter value. This module owns transport only.
 *
 * Every write runs inside a transition. The library's query key derives
 * from these params, and a suspense query suspends on a key it has no
 * data for; outside a transition that unwinds to the page's Suspense
 * boundary and repaints the whole browse surface as a skeleton on each
 * facet click. React holds the previous UI instead when the update is a
 * transition. This restores a property the surface used to inherit from
 * the router, whose navigations were transitions and whose loader
 * awaited the prefetch before the params changed; a shallow write runs
 * no loader. `placeholderData: keepPreviousData` is not an alternative
 * here, because suspense query options omit `placeholderData`.
 */
import {
  debounce,
  defaultRateLimit,
  parseAsNativeArrayOf,
  parseAsString,
  useQueryStates,
  type Nullable,
  type Values,
} from "nuqs";
import { useOptimisticSearchParams } from "nuqs/adapters/react-router/v8";
import { useTransition } from "react";

import { parseSortParam, serializeSortParam, type SortLevelParam } from "@/api";
import {
  parseFilterParams,
  serializeFilterParams,
  type FilterState,
} from "@/routes/library-params";

/** Debounce before a typed filter edit becomes a URL change. */
export const FILTER_DEBOUNCE_MS = 250;
/** Minimum quick-search length; below it the input clears the `q` filter. */
export const MIN_Q_LEN = 2;

/**
 * The library's search params as one bag, for readers that need the whole
 * grammar (wire params, active-filter count, chips).
 *
 * React Router's own `useSearchParams` is NOT interchangeable with this:
 * search-param writes reach the URL through `history.replaceState`, which
 * the router's history does not observe, so its view of the params goes
 * stale the moment anything here writes. Every library read must come
 * through this hook.
 */
export function useLibrarySearchParams(): URLSearchParams {
  return useOptimisticSearchParams();
}

/**
 * `cursor` rides along with every filter write: a changed condition
 * invalidates the keyset position the cursor names, so it is dropped in
 * the same update rather than left to a follow-up write that could be
 * observed in between.
 */
const QUICK_SEARCH_KEYS = {
  q: parseAsString,
  cursor: parseAsString,
};

export type QuickSearchFilter = {
  /** The committed `q` condition; `null` when search is not filtering. */
  query: string | null;
  /** Debounced write, for the typing path. */
  setQuery: (next: string | null) => void;
  /**
   * Immediate write, for clear affordances. Being undebounced is what
   * cancels a pending {@link QuickSearchFilter.setQuery}, so a queued
   * keystroke cannot resurrect the condition this call removes.
   */
  clearQuery: () => void;
};

/** The typing path for `q`. The only other writers are the whole-grammar
 *  clear affordances below, which sweep every filter key including this one. */
export function useQuickSearchFilter(): QuickSearchFilter {
  const [, startTransition] = useTransition();
  const [{ q }, setParams] = useQueryStates(QUICK_SEARCH_KEYS, { startTransition });
  return {
    query: q,
    setQuery: (next) => {
      void setParams({ q: next, cursor: null }, { limitUrlUpdates: debounce(FILTER_DEBOUNCE_MS) });
    },
    clearQuery: () => {
      void setParams({ q: null, cursor: null }, { limitUrlUpdates: defaultRateLimit });
    },
  };
}

/**
 * Project raw input text onto the committed `q` condition: trimmed, and
 * `null` below {@link MIN_Q_LEN} so a one-character box does not narrow
 * the library to nothing while the user is still typing.
 */
export function projectQuery(text: string): string | null {
  const trimmed = text.trim();
  return trimmed.length >= MIN_Q_LEN ? trimmed : null;
}

/** Keys the codec writes with `append`, so they carry repeated values. */
const MULTI = parseAsNativeArrayOf(parseAsString);

/**
 * Every key any library gesture may write, including the dead text
 * params that no longer have a wire predicate: a hand-crafted or stale
 * URL can still carry one, and clear-all has to be able to purge it.
 */
const LIBRARY_PARSERS = {
  q: parseAsString,
  cursor: parseAsString,
  sort: parseAsString,
  view: parseAsString,
  series: parseAsString,
  shelf: parseAsString,
  title_contains: parseAsString,
  title_eq: parseAsString,
  title_ne: parseAsString,
  title_empty: parseAsString,
  subtitle_contains: parseAsString,
  subtitle_empty: parseAsString,
  subtitle_eq: parseAsString,
  subtitle_ne: parseAsString,
  isbn_13_contains: parseAsString,
  isbn_13_eq: parseAsString,
  isbn_13_empty: parseAsString,
  isbn_13_ne: parseAsString,
  pages_gte: parseAsString,
  pages_lte: parseAsString,
  pages_empty: parseAsString,
  rating_gte: parseAsString,
  rating_lte: parseAsString,
  rating_empty: parseAsString,
  created_at_gte: parseAsString,
  created_at_lte: parseAsString,
  status_any: MULTI,
  status_none: MULTI,
  author: MULTI,
  author_any: MULTI,
  author_none: MULTI,
  tag: MULTI,
  tag_any: MULTI,
  tag_none: MULTI,
  genre: MULTI,
  genre_any: MULTI,
  genre_none: MULTI,
  mood: MULTI,
  mood_any: MULTI,
  mood_none: MULTI,
};

type LibraryKey = keyof typeof LIBRARY_PARSERS;

/**
 * The URL keys each editable slice owns. A gesture writes its slice and
 * nothing else, which is what keeps a sibling commit out of another
 * slice's debounce queue; see this module's header.
 */
export const SLICE_KEYS = {
  title: ["title_contains", "title_eq", "title_ne"],
  subtitle: ["subtitle_contains", "subtitle_empty"],
  isbn13: ["isbn_13_contains", "isbn_13_eq", "isbn_13_empty"],
  pages: ["pages_gte", "pages_lte", "pages_empty"],
  rating: ["rating_gte", "rating_lte", "rating_empty"],
  added: ["created_at_gte", "created_at_lte"],
  status: ["status_any", "status_none"],
  authors: ["author", "author_any", "author_none"],
  tags: ["tag", "tag_any", "tag_none"],
  genres: ["genre", "genre_any", "genre_none"],
  moods: ["mood", "mood_any", "mood_none"],
  series: ["series"],
  shelf: ["shelf"],
} as const satisfies Record<string, readonly LibraryKey[]>;

export type FilterSlice = keyof typeof SLICE_KEYS;

/**
 * Every key clear-all sweeps: each slice's keys plus the dead params.
 *
 * This is a second census of a list the codec already owns, and the two
 * must agree exactly or a condition becomes unclearable while still
 * being parsed and sent. The type only proves each entry is a key this
 * module can write; it cannot prove the list is complete against the
 * codec, so a parity test holds that end and is the thing to fix first
 * if it fails. Deriving this from `FILTER_PARAM_KEYS` instead would need
 * a cast (that list is `readonly string[]`) or a runtime narrowing that
 * would silently drop an unrecognised key, turning a loud drift into an
 * invisible one.
 */
export const ALL_FILTER_KEYS: readonly LibraryKey[] = [
  "q",
  ...Object.values(SLICE_KEYS).flat(),
  "title_empty",
  "subtitle_eq",
  "subtitle_ne",
  "isbn_13_ne",
];

type LibraryUpdate = Partial<Nullable<Values<typeof LIBRARY_PARSERS>>>;

/** The repeated-value keys, split out so an update stays precisely typed:
 *  these carry `string[] | null`, every other key `string | null`. */
const MULTI_KEYS = [
  "status_any",
  "status_none",
  "author",
  "author_any",
  "author_none",
  "tag",
  "tag_any",
  "tag_none",
  "genre",
  "genre_any",
  "genre_none",
  "mood",
  "mood_any",
  "mood_none",
] as const;

type MultiKey = (typeof MULTI_KEYS)[number];

function isMultiKey(key: LibraryKey): key is MultiKey {
  return MULTI_KEYS.some((candidate) => candidate === key);
}

/**
 * Render the given keys of `next` as a URL update, by serialising through
 * the codec and reading back only those keys. Going through the codec
 * rather than writing values directly keeps one serialiser in the
 * codebase; a key absent from the serialised output is written `null`,
 * which deletes it.
 */
function updateFor(keys: readonly LibraryKey[], next: FilterState): LibraryUpdate {
  const serialised = new URLSearchParams();
  serializeFilterParams(next, serialised);
  const update: LibraryUpdate = {};
  for (const key of keys) {
    if (isMultiKey(key)) {
      const values = serialised.getAll(key);
      update[key] = values.length === 0 ? null : values;
    } else {
      update[key] = serialised.get(key);
    }
  }
  return update;
}

/**
 * Rebuild a `URLSearchParams` from the pending values, so the codec stays
 * the only thing that parses the grammar. Callers get the value a debounced
 * write is still holding, which is what an editing surface must render;
 * anything that should wait for the write to settle (the API query, the
 * chips, the active count) reads {@link useLibrarySearchParams} instead.
 */
function toSearch(values: Values<typeof LIBRARY_PARSERS>): URLSearchParams {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) {
    if (value === null) continue;
    if (Array.isArray(value)) for (const item of value) search.append(key, item);
    else search.set(key, value);
  }
  return search;
}

export type LibraryFilters = {
  /**
   * The grammar as the editing surfaces see it, including a value a
   * debounced write is still holding.
   */
  filters: FilterState;
  /**
   * Write one slice. `debounced` is for typed inputs; leaving it off
   * makes the write immediate, which also cancels any debounced write
   * still queued against the same keys.
   */
  commitSlice: (
    slice: FilterSlice,
    patch: (current: FilterState) => FilterState,
    options?: { debounced?: boolean },
  ) => void;
  /** The committed sort stack. */
  sortLevels: readonly SortLevelParam[];
  /** Replace the sort stack. Ordering is not a filter, but it invalidates
   *  the cursor the same way. */
  commitSort: (levels: readonly SortLevelParam[]) => void;
  /**
   * Write the whole grammar at once, for the clear affordances that are not
   * scoped to a slice (chip removal). Never debounced, so it cancels any
   * queued write. It patches the applied values rather than the pending
   * ones, so a slice mid-edit is discarded rather than published early.
   */
  commitAll: (patch: (current: FilterState) => FilterState) => void;
  /** Drop every filter condition, dead params included. Never debounced. */
  clearAll: () => void;
  /**
   * Discard whatever the typed slices are holding, by writing the applied
   * URL's values back over their keys. Undebounced, so it cancels their
   * pending writes; this is how the drawer's Escape abandons an edit
   * without any surface tracking that an abandon happened. Only the typed
   * slices can hold anything: every other gesture writes immediately.
   */
  revertTypedEdits: () => void;
  /** Set the browse projection. Not a filter, so it leaves the cursor. */
  setView: (view: string | null) => void;
};

/** The slices whose writes are debounced, so the only ones that can be
 *  holding a value the URL has not taken yet. */
const TYPED_SLICES: readonly FilterSlice[] = ["title", "subtitle", "isbn13", "pages", "rating"];

/** The library's filter grammar. Every library write goes through this. */
export function useLibraryFilters(): LibraryFilters {
  const applied = useLibrarySearchParams();
  const [, startTransition] = useTransition();
  const [values, setParams] = useQueryStates(LIBRARY_PARSERS, { startTransition });
  const filters = parseFilterParams(toSearch(values));
  return {
    filters,
    sortLevels: parseSortParam(values.sort ?? ""),
    commitSlice: (slice, patch, options) => {
      void setParams(
        // `cursor` rides along because a changed condition invalidates the
        // keyset position it names.
        { ...updateFor(SLICE_KEYS[slice], patch(filters)), cursor: null },
        {
          limitUrlUpdates:
            options?.debounced === true ? debounce(FILTER_DEBOUNCE_MS) : defaultRateLimit,
        },
      );
    },
    commitSort: (levels) => {
      const serialised = serializeSortParam(levels);
      void setParams(
        { sort: serialised === "" ? null : serialised, cursor: null },
        { limitUrlUpdates: defaultRateLimit },
      );
    },
    commitAll: (patch) => {
      // Patches the APPLIED grammar, not the pending one. Patching pending
      // values would publish a half-typed sibling slice to the URL and the
      // wire: being undebounced cancels that slice's queued write, but it
      // carries the same value out early rather than dropping it.
      void setParams(
        { ...updateFor(ALL_FILTER_KEYS, patch(parseFilterParams(applied))), cursor: null },
        { limitUrlUpdates: defaultRateLimit },
      );
    },
    clearAll: () => {
      const cleared: LibraryUpdate = { cursor: null };
      for (const key of ALL_FILTER_KEYS) cleared[key] = null;
      void setParams(cleared, { limitUrlUpdates: defaultRateLimit });
    },
    revertTypedEdits: () => {
      const appliedFilters = parseFilterParams(applied);
      let update: LibraryUpdate = {};
      for (const slice of TYPED_SLICES) {
        update = { ...update, ...updateFor(SLICE_KEYS[slice], appliedFilters) };
      }
      void setParams(update, { limitUrlUpdates: defaultRateLimit });
    },
    setView: (view) => {
      void setParams({ view }, { limitUrlUpdates: defaultRateLimit });
    },
  };
}
