/**
 * Admin-only enrichment-failure API client
 * (`/api/v1/dashboard/enrichment-failures`) and the existing per-book
 * enrichment trigger.
 *
 * The shapes mirror `FailuresResponse` and `FailureCountsResponse` in
 * `backend/openapi.json`.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const FAILURE_CLASSES = [
  "timeout",
  "rate_limited",
  "source_error",
  "not_found",
  "unreachable",
  "internal",
  "unspecified",
] as const;

const FailureClassSchema = z.enum(FAILURE_CLASSES);
/** Why one metadata source, or the run itself, failed. */
export type FailureClass = z.infer<typeof FailureClassSchema>;

const FailureRefSchema = z.object({
  source: z.string().nullish(),
  class: FailureClassSchema,
});
/** One failure of a book's latest enrichment attempt. */
export type FailureRef = z.infer<typeof FailureRefSchema>;

const FailureItemSchema = z.object({
  manifestation_id: z.uuid(),
  work_id: z.uuid(),
  title: z.string(),
  status: z.enum(["pending", "in_progress", "complete", "failed", "skipped"]),
  attempt_count: z.number().int().nonnegative(),
  // `attempted_at` can hold a future retry anchor for rate-limited rows, so it
  // is validated but never rendered.
  attempted_at: z.iso.datetime().nullish(),
  primary: FailureRefSchema,
  also: z.array(FailureRefSchema),
});
/** One book whose enrichment is failing. Mirrors `FailureItem` on the wire. */
export type FailureItem = z.infer<typeof FailureItemSchema>;

const FailuresResponseSchema = z.object({
  items: z.array(FailureItemSchema),
  next_cursor: z.string().nullish(),
});
/** One page of failing books. Mirrors `FailuresResponse` on the wire. */
export type FailuresPage = z.infer<typeof FailuresResponseSchema>;

const FailureCountsSchema = z.object({
  by_failure: z.array(
    z.object({
      source: z.string().nullish(),
      class: FailureClassSchema,
      count: z.number().int().nonnegative(),
    }),
  ),
  total: z.number().int().nonnegative(),
});
/** Per-group counts. Mirrors `FailureCountsResponse` on the wire. */
export type FailureCounts = z.infer<typeof FailureCountsSchema>;

/** Default page size sent with every group request. */
const FAILURES_PAGE_SIZE = 25;

type ListFailuresParams = {
  /** Source key; absent for the groups that belong to no source. */
  source: string | null;
  class: FailureClass;
  cursor?: string;
  limit?: number;
};

/**
 * `GET /api/v1/dashboard/enrichment-failures` — one page of the books listed
 * under one failure group.
 */
async function listEnrichmentFailures(
  params: ListFailuresParams,
  signal?: AbortSignal,
): Promise<FailuresPage> {
  const query = new URLSearchParams({ class: params.class });
  if (params.source !== null) query.set("source", params.source);
  query.set("limit", String(params.limit ?? FAILURES_PAGE_SIZE));
  if (params.cursor !== undefined) query.set("cursor", params.cursor);
  const raw = await apiFetch(
    `/api/v1/dashboard/enrichment-failures?${query.toString()}`,
    signal ? { signal } : {},
  );
  return FailuresResponseSchema.parse(raw);
}

/** `GET /api/v1/dashboard/enrichment-failures/counts` — books per failure group. */
async function getEnrichmentFailureCounts(signal?: AbortSignal): Promise<FailureCounts> {
  const raw = await apiFetch(
    "/api/v1/dashboard/enrichment-failures/counts",
    signal ? { signal } : {},
  );
  return FailureCountsSchema.parse(raw);
}

/** `POST /api/v1/manifestations/{id}/enrichment/trigger` — queue a fresh enrichment pass. */
async function triggerEnrichment(manifestationId: string): Promise<void> {
  await apiFetch(
    `/api/v1/manifestations/${encodeURIComponent(manifestationId)}/enrichment/trigger`,
    {
      method: "POST",
    },
  );
}

export {
  FAILURE_CLASSES,
  FAILURES_PAGE_SIZE,
  listEnrichmentFailures,
  getEnrichmentFailureCounts,
  triggerEnrichment,
};
