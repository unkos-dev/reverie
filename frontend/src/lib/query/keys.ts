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
import type { ListBooksParams } from "@/api";

/** Tuple for the root books namespace. */
export type BooksAllKey = readonly ["books"];
/** Tuple for the books-list cache slot, keyed by filter/sort/cursor params. */
export type BooksListKey = readonly ["books", "list", ListBooksParams];
/** Tuple for the books-detail cache slot, keyed by manifestation id. */
export type BookDetailKey = readonly ["books", "detail", string];
/** Tuple for the works-detail cache slot, keyed by work id. */
export type WorkDetailKey = readonly ["works", "detail", string];
/** Tuple for the search cache slot, keyed by the trimmed query string. */
export type SearchKey = readonly ["search", string];

/** Read-only key arrays — `as const` makes them tuples for TS narrowing. */
export const queryKeys = {
  books: {
    /** Root namespace; invalidate to wipe every books-* cache slot. */
    all: ["books"] as const satisfies BooksAllKey,
    /** List endpoint with filters/sort/cursor. Distinct params = distinct slot. */
    list: (params: ListBooksParams): BooksListKey => ["books", "list", params] as const,
    /** Detail endpoint for one manifestation. */
    detail: (id: string): BookDetailKey => ["books", "detail", id] as const,
  },
  works: {
    /** Detail endpoint for one work + its visible manifestations. */
    detail: (id: string): WorkDetailKey => ["works", "detail", id] as const,
  },
  /** Search-results cache slot. Distinct trimmed `q` = distinct slot. */
  search: (q: string): SearchKey => ["search", q] as const,
  series: {
    /** Root namespace; invalidate to wipe every series-* cache slot. */
    all: ["series"] as const,
    /** Detail endpoint for one series. */
    detail: (id: string) => ["series", "detail", id] as const,
  },
  shelves: {
    /** Root namespace; invalidate to refetch every shelves-* slot. */
    all: ["shelves"] as const,
    /** `GET /api/shelves` list. */
    list: () => ["shelves", "list"] as const,
    /** Single shelf detail (with items). */
    detail: (id: string) => ["shelves", "detail", id] as const,
  },
  users: {
    /** Root namespace; invalidate to refetch every users-* slot. */
    all: ["users"] as const,
    /** `GET /api/users` list (admin only). */
    list: () => ["users", "list"] as const,
  },
  auth: {
    /** `/auth/me` — current authenticated user identity. */
    me: () => ["auth", "me"] as const,
  },
  dashboard: {
    /** Root namespace; invalidate to refetch every dashboard-* slot. */
    all: ["dashboard"] as const,
    /** `GET /api/dashboard/stats` aggregate metrics (admin only). */
    stats: () => ["dashboard", "stats"] as const,
    /** `GET /api/dashboard/activity` recent batches, keyed by limit. */
    activity: (limit: number) => ["dashboard", "activity", limit] as const,
  },
};
