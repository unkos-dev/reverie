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

import { ResourceEtagSchema, ReadingStatusSchema, type ReadingStatus } from "./books";
import { apiFetchWithHeaders } from "./fetch";

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

export type ReadingSnapshot = { values: ReadingState; etag: string };

export async function getReadingState(id: string, signal?: AbortSignal): Promise<ReadingSnapshot> {
  const { body, headers } = await apiFetchWithHeaders(
    `/api/v1/books/${encodeURIComponent(id)}/reading`,
    { signal },
  );
  const values = ReadingStateSchema.parse(body);
  return { values, etag: ResourceEtagSchema.parse(headers.get("ETag")) };
}

export async function updateReadingState(
  id: string,
  fields: UpdateReadingFields,
  baseline: string,
  signal?: AbortSignal,
): Promise<ReadingSnapshot> {
  const { body, headers } = await apiFetchWithHeaders(
    `/api/v1/books/${encodeURIComponent(id)}/reading`,
    {
      method: "PATCH",
      headers: { "If-Match": ResourceEtagSchema.parse(baseline) },
      body: JSON.stringify(fields),
      signal,
    },
  );
  const values = ReadingStateSchema.parse(body);
  return { values, etag: ResourceEtagSchema.parse(headers.get("ETag")) };
}
