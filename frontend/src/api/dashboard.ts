/**
 * Admin-only library-health dashboard API client (`/api/dashboard/*`).
 *
 * Both endpoints require `role = admin`; non-admin callers receive 403.
 * Response bodies are parsed through Zod schemas at the boundary (per
 * `frontend/CLAUDE.md`) — the shapes mirror the DTOs in
 * `backend/src/routes/dashboard/mod.rs`.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const StatusCountSchema = z.object({
  status: z.string(),
  count: z.number().int().nonnegative(),
});

const FormatBucketSchema = z.object({
  format: z.string(),
  count: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
});

const MetadataCoverageSchema = z.object({
  total: z.number().int().nonnegative(),
  has_description: z.number().int().nonnegative(),
  has_language: z.number().int().nonnegative(),
  has_isbn_13: z.number().int().nonnegative(),
  has_cover: z.number().int().nonnegative(),
});

const DashboardStatsSchema = z.object({
  total_manifestations: z.number().int().nonnegative(),
  total_works: z.number().int().nonnegative(),
  storage_total_bytes: z.number().int().nonnegative(),
  storage_cover_bytes: z.number().int().nonnegative(),
  storage_by_format: z.array(FormatBucketSchema),
  validation_breakdown: z.array(StatusCountSchema),
  clean_non_epub_count: z.number().int().nonnegative(),
  enrichment_breakdown: z.array(StatusCountSchema),
  metadata_coverage: MetadataCoverageSchema,
});
/** Aggregate library-health metrics. Mirrors `StatsResponse` on the wire. */
export type DashboardStats = z.infer<typeof DashboardStatsSchema>;

const BatchRowSchema = z.object({
  batch_id: z.uuid(),
  started_at: z.string(),
  ended_at: z.string().nullable(),
  total: z.number().int().nonnegative(),
  completed: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
  skipped: z.number().int().nonnegative(),
  in_progress: z.number().int().nonnegative(),
});
/** One recent ingestion batch. Mirrors `BatchRow` on the wire. */
export type BatchRow = z.infer<typeof BatchRowSchema>;

const DashboardActivitySchema = z.object({
  batches: z.array(BatchRowSchema),
});
/** Recent ingestion activity. Mirrors `ActivityResponse` on the wire. */
export type DashboardActivity = z.infer<typeof DashboardActivitySchema>;

/** `GET /api/dashboard/stats` — library-wide aggregate metrics (admin only). */
async function getDashboardStats(signal?: AbortSignal): Promise<DashboardStats> {
  const raw = await apiFetch("/api/dashboard/stats", signal ? { signal } : {});
  return DashboardStatsSchema.parse(raw);
}

/**
 * `GET /api/dashboard/activity?limit=N` — most-recent ingestion batches
 * (admin only). `limit` is clamped server-side to `1..=100` (default 20).
 */
async function getDashboardActivity(
  limit?: number,
  signal?: AbortSignal,
): Promise<DashboardActivity> {
  const path =
    limit === undefined
      ? "/api/dashboard/activity"
      : `/api/dashboard/activity?limit=${String(limit)}`;
  const raw = await apiFetch(path, signal ? { signal } : {});
  return DashboardActivitySchema.parse(raw);
}

export { getDashboardStats, getDashboardActivity };
