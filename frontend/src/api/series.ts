/**
 * Read-only client for `/api/series/{id}` (sub-phase 11d).
 *
 * Mirrors the response DTO in `backend/src/models/series.rs`. Snake_case
 * field names match the wire shape; the Zod schema is the boundary
 * guard per `frontend/CLAUDE.md`.
 *
 * The frontend computes the "completeness" indicator (`own / total`)
 * from the response — `total = works.length`, `own = works.filter(w =>
 * w.manifestations.length > 0).length`. Works the user has no copy of
 * surface with an empty `manifestations` array.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const WorkManifestationSchema = z.object({
  id: z.string(),
  isbn_13: z.string().nullable(),
  isbn_10: z.string().nullable(),
  cover_url: z.string(),
  ingestion_status: z.enum(["pending", "processing", "complete", "failed", "skipped"]),
  validation_status: z.string(),
  enrichment_status: z.enum(["pending", "in_progress", "complete", "failed", "skipped"]),
  created_at: z.string(),
});
/** One manifestation row attached to a work in a series response. */
export type SeriesWorkManifestation = z.infer<typeof WorkManifestationSchema>;

const SeriesWorkSchema = z.object({
  id: z.string(),
  title: z.string(),
  authors: z.array(z.string()),
  position: z.number().nullable(),
  manifestations: z.array(WorkManifestationSchema),
});
/** One work in a series. Empty `manifestations` array = the user owns no copy ("gap"). */
export type SeriesWork = z.infer<typeof SeriesWorkSchema>;

const SeriesDetailSchema = z.object({
  id: z.string(),
  name: z.string(),
  sort_name: z.string(),
  works: z.array(SeriesWorkSchema),
});
/** `GET /api/series/{id}` response envelope. */
export type SeriesDetail = z.infer<typeof SeriesDetailSchema>;

/**
 * Fetch a series detail by id. RLS-hidden rows resolve to 404
 * (existence-not-leaked).
 */
export async function getSeries(id: string, signal?: AbortSignal): Promise<SeriesDetail> {
  const body = await apiFetch(
    `/api/series/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
  return SeriesDetailSchema.parse(body);
}
