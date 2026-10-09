/**
 * Admin-only ingestion API client (`/api/v1/ingestion/*`).
 *
 * The shapes mirror `InputsResponse`, `InputCountsResponse` and
 * `DiscoveryResult` in `backend/openapi.json`. Every endpoint requires
 * `role = admin`; other callers receive 403.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

/**
 * Closed reason classes, most severe first. The order is the precedence the
 * server uses to pick a file's primary reason, so it doubles as the display
 * order of the groups.
 */
const INPUT_REASONS = [
  "unsafe_contents",
  "damaged",
  "invalid_structure",
  "over_limits",
  "unspecified",
  "needs_change",
  "retries_exhausted",
  "format_not_accepted",
] as const;

const InputReasonSchema = z.enum(INPUT_REASONS);
/** One closed reason class an input is listed or counted under. */
export type InputReason = z.infer<typeof InputReasonSchema>;

const InputStatusSchema = z.enum([
  "pending",
  "processing",
  "imported",
  "duplicate",
  "rejected",
  "not_accepted",
  "operational_failure",
  "removed",
]);

const AttemptOutcomeSchema = z.enum([
  "imported",
  "duplicate",
  "rejected",
  "changed",
  "shared_dependency",
  "transient_input",
  "needs_change",
  "interrupted",
]);

const InputItemSchema = z.object({
  id: z.uuid(),
  path: z.string(),
  status: InputStatusSchema,
  outcome: AttemptOutcomeSchema.nullish(),
  primary_reason: InputReasonSchema,
  reasons: z.array(InputReasonSchema),
  observed_at: z.iso.datetime(),
  completed_at: z.iso.datetime().nullish(),
});
/** One input needing attention. Mirrors `InputItem` on the wire. */
export type InputItem = z.infer<typeof InputItemSchema>;

const InputsResponseSchema = z.object({
  items: z.array(InputItemSchema),
  next_cursor: z.string().nullish(),
});
/** One page of inputs. Mirrors `InputsResponse` on the wire. */
export type InputsPage = z.infer<typeof InputsResponseSchema>;

const InputCountsSchema = z.object({
  by_reason: z.array(
    z.object({
      reason: InputReasonSchema,
      count: z.number().int().nonnegative(),
    }),
  ),
  attention_total: z.number().int().nonnegative(),
});
/** Per-class counts. Mirrors `InputCountsResponse` on the wire. */
export type InputCounts = z.infer<typeof InputCountsSchema>;

const DiscoveryResultSchema = z.object({
  queued: z.number().int().nonnegative(),
  deferred: z.number().int().nonnegative(),
  suppressed: z.number().int().nonnegative(),
  monitor: z.string(),
});
/** Classification counts from a scan. Mirrors `DiscoveryResult` on the wire. */
export type DiscoveryResult = z.infer<typeof DiscoveryResultSchema>;

/** Default page size sent with every group request. */
const INPUTS_PAGE_SIZE = 25;

type ListInputsParams = {
  reason: InputReason;
  cursor?: string;
  limit?: number;
};

/**
 * `GET /api/v1/ingestion/inputs?reason=<class>` — one page of the files listed
 * under one primary reason class.
 */
async function listInputs(params: ListInputsParams, signal?: AbortSignal): Promise<InputsPage> {
  const query = new URLSearchParams({ reason: params.reason });
  query.set("limit", String(params.limit ?? INPUTS_PAGE_SIZE));
  if (params.cursor !== undefined) query.set("cursor", params.cursor);
  const raw = await apiFetch(
    `/api/v1/ingestion/inputs?${query.toString()}`,
    signal ? { signal } : {},
  );
  return InputsResponseSchema.parse(raw);
}

/** `GET /api/v1/ingestion/inputs/counts` — files per primary reason class. */
async function getInputCounts(signal?: AbortSignal): Promise<InputCounts> {
  const raw = await apiFetch("/api/v1/ingestion/inputs/counts", signal ? { signal } : {});
  return InputCountsSchema.parse(raw);
}

/** `POST /api/v1/ingestion/scan` — rediscover the ingestion folder. */
async function scanIngestion(): Promise<DiscoveryResult> {
  const raw = await apiFetch("/api/v1/ingestion/scan", { method: "POST" });
  return DiscoveryResultSchema.parse(raw);
}

export { INPUT_REASONS, INPUTS_PAGE_SIZE, listInputs, getInputCounts, scanIngestion };
