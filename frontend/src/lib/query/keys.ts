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

/** Read-only key arrays — `as const` makes them tuples for TS narrowing. */
export const queryKeys = {
  books: {
    /** Root namespace; invalidate to wipe every books-* cache slot. */
    all: ["books"] as const,
    /** List endpoint with filters/sort/cursor. Distinct params = distinct slot. */
    list: (params: ListBooksParams) => ["books", "list", params] as const,
    /** Detail endpoint for one manifestation. */
    detail: (id: string) => ["books", "detail", id] as const,
  },
  works: {
    /** Detail endpoint for one work + its visible manifestations. */
    detail: (id: string) => ["works", "detail", id] as const,
  },
};
