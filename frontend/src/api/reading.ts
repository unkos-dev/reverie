/**
 * Client for the `/api/v1/books/{id}/reading` per-user reading-state
 * surface: status, rating, notes, progress, and reading dates.
 *
 * Mirrors `ReadingState` in `backend/src/models/reading_state.rs`.
 * Runtime validation follows the same boundary convention as
 * `./books.ts`: `apiFetch` returns `unknown`, and the schema here
 * converts it into the strongly-typed shape the UI consumes.
 */
import { z } from "zod";

import { ReadingStatusSchema, type ReadingStatus } from "./books";
import { apiFetch } from "./fetch";

export const ReadingStateSchema = z.object({
  status: ReadingStatusSchema.nullable(),
  rating: z.number().int().nullable(),
  notes: z.string().nullable(),
  progress_pct: z.number().nullable(),
  started_at: z.iso.datetime().nullable(),
  finished_at: z.iso.datetime().nullable(),
  last_read_at: z.iso.datetime().nullable(),
});
/**
 * A caller's reading state for one book. All-null fields mean unread
 * (no row yet). Mirrors the `GET`/`PATCH /api/v1/books/{id}/reading`
 * response body.
 */
export type ReadingState = z.infer<typeof ReadingStateSchema>;

/**
 * RFC 7396 JSON Merge Patch body for `PATCH /api/v1/books/{id}/reading`.
 * A key omitted leaves the field unchanged; a key present with `null`
 * clears it. `progress_pct` and the timestamp fields are server-derived
 * from transition stamps and are not accepted here.
 */
export type UpdateReadingFields = {
  status?: ReadingStatus | null;
  rating?: number | null;
  notes?: string | null;
};

/**
 * Fetch the caller's reading state for one book. All-null fields mean
 * unread (no row yet). Callers that need a fresh representation before
 * editing (a grid cell entering edit mode) should call this rather than
 * rely on a cached row, since only this response's `ETag` is guaranteed
 * current for a following `PATCH`'s `If-Match`.
 *
 * Throws an `ApiError` with `status === 404` when the manifestation is
 * missing or RLS-hidden (existence-not-leaked).
 */
export async function getReadingState(id: string, signal?: AbortSignal): Promise<ReadingState> {
  const body = await apiFetch(
    `/api/v1/books/${encodeURIComponent(id)}/reading`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
  return ReadingStateSchema.parse(body);
}

/**
 * Update the caller's reading state for one book. The body is a bare
 * RFC 7396 merge patch, no envelope, matching the wire shape of
 * `UpdateReadingRequest` on the backend.
 *
 * Throws an `ApiError` with `status === 422` when the body has no
 * populated fields, `rating` is outside `1..=5`, or `notes` exceeds
 * 10000 characters; `status === 404` when the manifestation is
 * missing or RLS-hidden (existence-not-leaked).
 *
 * On success the server returns 200 with the full post-merge row,
 * including any transition-stamp side effects (e.g. `started_at` set
 * on first `reading` status).
 */
export async function updateReadingState(
  id: string,
  fields: UpdateReadingFields,
  signal?: AbortSignal,
): Promise<ReadingState> {
  const init: RequestInit = {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
    ...(signal ? { signal } : {}),
  };
  const body = await apiFetch(`/api/v1/books/${encodeURIComponent(id)}/reading`, init);
  return ReadingStateSchema.parse(body);
}
