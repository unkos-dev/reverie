/**
 * Centralised query-key factory.
 *
 * Mirrors the "query key factory" pattern documented by Tkdodo: keys
 * are derived from one place so that invalidation, prefetch, and
 * useQuery all share the same shape, and a typo lands in TypeScript
 * rather than at runtime as a silently-missed cache entry.
 *
 * Keys are tuples of literal strings then parameters. Parameter
 * objects are passed in by reference — react-query handles structural
 * equality on the key, so equal params hit the same cache slot.
 */
import type { ArrayParamKey, ListBooksParams } from "@/api";
import type { SuggestKind } from "@/api/suggest";

/** Tuple for the root books namespace. */
export type BooksAllKey = readonly ["books"];
/** Tuple for the books-list cache slot, keyed by filter/sort/cursor params. */
export type BooksListKey = readonly ["books", "list", ListBooksParams];
/** Tuple prefix shared by every books-detail cache slot. */
export type BookDetailsAllKey = readonly ["books", "detail"];
/** Tuple for the books-detail cache slot, keyed by manifestation id. */
export type BookDetailKey = readonly ["books", "detail", string];
/** Tuple for the works-detail cache slot, keyed by work id. */
export type WorkDetailKey = readonly ["works", "detail", string];
/** Tuple for the search cache slot, keyed by the trimmed query string. */
export type SearchKey = readonly ["search", string];

/** Locale-independent string order (codepoint), so a set of ids sorts to the
 *  same sequence regardless of the caller's locale and hits one cache slot. */
function byCodepoint(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

/**
 * Identity that also proves the list complete: when any `ArrayParamKey` is
 * missing, the `Exclude` is non-`never` and the parameter type collapses to
 * `never`, failing compilation. Completeness matters because a key this list
 * misses silently skips normalization and re-fragments the cache by value
 * order; the element constraint rejects bogus keys in the other direction.
 */
function exhaustiveArrayParamKeys<K extends readonly ArrayParamKey[]>(
  keys: K & ([Exclude<ArrayParamKey, K[number]>] extends [never] ? unknown : never),
): readonly ArrayParamKey[] {
  return keys;
}

const ARRAY_PARAM_KEYS = exhaustiveArrayParamKeys([
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
  "status_any",
  "status_none",
] as const);

/**
 * Sort every array-valued filter param with the same locale-independent
 * comparator `authors.resolve` uses. Two filter sets that differ only in
 * the order their multi-value params were appended to the URL (e.g. tag
 * chips added in a different sequence) must hit the same cache slot rather
 * than fragmenting into two.
 */
function normalizeListParams(params: ListBooksParams): ListBooksParams {
  const normalized: ListBooksParams = { ...params };
  for (const key of ARRAY_PARAM_KEYS) {
    const value = normalized[key];
    if (value !== undefined) normalized[key] = [...value].sort(byCodepoint);
  }
  return normalized;
}

/** Read-only key arrays — `as const` makes them tuples for TS narrowing. */
export const queryKeys = {
  books: {
    /** Root namespace; invalidate to wipe every books-* cache slot. */
    all: ["books"] as const satisfies BooksAllKey,
    /** List endpoint with filters/sort/cursor. Distinct params = distinct slot. */
    list: (params: ListBooksParams): BooksListKey =>
      ["books", "list", normalizeListParams(params)] as const,
    /** Prefix of the whole detail family; invalidate to mark every cached
     *  book detail stale (refetches only the ones currently on screen). */
    detailsAll: ["books", "detail"] as const satisfies BookDetailsAllKey,
    /** Detail endpoint for one manifestation. */
    detail: (id: string): BookDetailKey => ["books", "detail", id] as const,
  },
  works: {
    /** Detail endpoint for one work + its visible manifestations. */
    detail: (id: string): WorkDetailKey => ["works", "detail", id] as const,
  },
  /** Search-results cache slot. Distinct trimmed `q` = distinct slot. */
  search: (q: string): SearchKey => ["search", q] as const,
  suggest: {
    /** One vocabulary's typeahead rows, keyed by kind + trimmed query. */
    vocab: (kind: SuggestKind, q: string) => ["suggest", kind, q] as const,
  },
  authors: {
    /** Author id → label resolution for chip hydration. Ids are sorted with an
     *  explicit, locale-independent comparator so the same set hits one cache
     *  slot regardless of the order they appear in the URL. */
    resolve: (ids: readonly string[]) =>
      ["authors", "resolve", [...ids].sort(byCodepoint)] as const,
  },
  series: {
    /** Root namespace; invalidate to wipe every series-* cache slot. */
    all: ["series"] as const,
    /** Detail endpoint for one series. */
    detail: (id: string) => ["series", "detail", id] as const,
  },
  shelves: {
    /** Root namespace; invalidate to refetch every shelves-* slot. */
    all: ["shelves"] as const,
    /** `GET /api/v1/shelves` list. */
    list: () => ["shelves", "list"] as const,
    /** Single shelf detail (with items). */
    detail: (id: string) => ["shelves", "detail", id] as const,
  },
  users: {
    /** Root namespace; invalidate to refetch every users-* slot. */
    all: ["users"] as const,
    /** `GET /api/v1/users` list (admin only). */
    list: () => ["users", "list"] as const,
  },
  auth: {
    /** `/auth/me` — current authenticated user identity. */
    me: () => ["auth", "me"] as const,
    /** `/auth/setup/status` — first-run + provider state for the auth screens and the redirect. */
    setupStatus: () => ["auth", "setup-status"] as const,
  },
  tokens: {
    /** Root namespace; invalidate to refetch every tokens-* slot. */
    all: ["tokens"] as const,
    /** `GET /api/v1/tokens` list (the caller's own tokens). */
    list: () => ["tokens", "list"] as const,
  },
  dashboard: {
    /** Root namespace; invalidate to refetch every dashboard-* slot. */
    all: ["dashboard"] as const,
    /** `GET /api/v1/dashboard/stats` aggregate metrics (admin only). */
    stats: () => ["dashboard", "stats"] as const,
    /** `GET /api/v1/dashboard/activity` recent batches, keyed by limit. */
    activity: (limit: number) => ["dashboard", "activity", limit] as const,
  },
};
