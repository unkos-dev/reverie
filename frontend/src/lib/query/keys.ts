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
import type { FailureClass } from "@/api/enrichment-failures";
import type { InputReason } from "@/api/ingestion";

/** Read-only key arrays for the remaining HTTP application data. */
export const queryKeys = {
  users: {
    linkEmail: (id: string, jobId: string) => ["users", id, "link-email", jobId] as const,
    /** Root namespace; invalidate to refetch every users-* slot. */
    all: ["users"] as const,
    /** `GET /api/v1/users` list (admin only). */
    list: () => ["users", "list"] as const,
  },
  auth: {
    emailVerification: () => ["auth", "email-verification"] as const,
    /** `/auth/me` — current authenticated user identity. */
    me: () => ["auth", "me"] as const,
    /** `/auth/setup/status` — first-run + provider state for the auth screens and the redirect. */
    setupStatus: () => ["auth", "setup-status"] as const,
  },
  settings: {
    /** Root namespace; invalidate to refetch every settings-* slot. */
    all: ["settings"] as const,
    /** `GET /api/v1/settings` snapshot (admin only); its ETag guards the PUT. */
    snapshot: () => ["settings", "snapshot"] as const,
    /** `GET /api/v1/settings/reload-status` (admin only); no ETag, safe to refetch. */
    reloadStatus: () => ["settings", "reload-status"] as const,
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
  ingestion: {
    status: () => ["ingestion", "status"] as const,
    /** Root namespace; invalidate to refetch every ingestion-* slot. */
    all: ["ingestion"] as const,
    /** `GET /api/v1/ingestion/inputs/counts`, one read for every group header. */
    counts: () => ["ingestion", "counts"] as const,
    /** Prefix of every group's first page; invalidating it never touches later pages. */
    groupsFirst: ["ingestion", "inputs"] as const,
    /** One group's first page (`reason=<class>`, no cursor). */
    groupFirst: (reason: InputReason) => ["ingestion", "inputs", reason] as const,
    groupAfterPrefix: (reason: InputReason) => ["ingestion", "after", reason] as const,
    /** One later page of a group, keyed by the cursor that opens it. */
    groupAfter: (reason: InputReason, cursor: string) =>
      ["ingestion", "after", reason, cursor] as const,
  },
  enrichmentFailures: {
    /** Root namespace; invalidate to refetch every enrichment-failure slot. */
    all: ["enrichment-failures"] as const,
    /** `GET /api/v1/dashboard/enrichment-failures/counts`. */
    counts: () => ["enrichment-failures", "counts"] as const,
    /** Prefix of every group's first page. */
    groupsFirst: ["enrichment-failures", "list"] as const,
    /** One group's first page; a null source is a group that belongs to no source. */
    groupFirst: (source: string | null, failureClass: FailureClass) =>
      ["enrichment-failures", "list", source ?? "", failureClass] as const,
    groupAfterPrefix: (source: string | null, failureClass: FailureClass) =>
      ["enrichment-failures", "after", source ?? "", failureClass] as const,
    /** One later page of a group, keyed by the cursor that opens it. */
    groupAfter: (source: string | null, failureClass: FailureClass, cursor: string) =>
      ["enrichment-failures", "after", source ?? "", failureClass, cursor] as const,
  },
};
