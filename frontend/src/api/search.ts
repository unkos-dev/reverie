/**
 * Client for the `/api/v1/search?q=` endpoint (11b).
 *
 * Mirrors `SearchResponse` / `SearchHit` in
 * `backend/src/models/library.rs`. Snake_case field names match the
 * wire shape — no transform on the frontend. Runtime parsed through
 * Zod so a schema-violating response surfaces as `ZodError` instead of
 * silently corrupting downstream state.
 *
 * The `snippet` field carries ASCII `STX` (0x02) / `ETX` (0x03)
 * markers from Postgres `ts_headline` (non-HTML start/stop delimiters
 * — Unicode-reserved control codepoints that cannot legally appear in
 * valid UTF-8 text, so they cannot collide with user typography).
 * Render via the `<HighlightedSnippet>` helper rather than
 * `dangerouslySetInnerHTML`.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const SearchHitKindSchema = z.enum(["book"]);
/** Tag identifying which entity a {@link SearchHit} points at. */
export type SearchHitKind = z.infer<typeof SearchHitKindSchema>;

const SearchHitSchema = z.object({
  kind: SearchHitKindSchema,
  id: z.string(),
  work_id: z.string().nullable(),
  title: z.string(),
  authors: z.array(z.string()),
  snippet: z.string().nullable(),
  cover_url: z.string().nullable(),
});
/** One result row from `GET /api/v1/search`. Discriminate by `kind`. */
export type SearchHit = z.infer<typeof SearchHitSchema>;

const SearchResponseSchema = z.object({
  items: z.array(SearchHitSchema),
});
/** Envelope returned by `GET /api/v1/search`. */
export type SearchResponse = z.infer<typeof SearchResponseSchema>;

/**
 * Search the library by free-form query. Hybrid full-text + trigram
 * results, ranked DESC. 20-row top-N — no pagination yet.
 *
 * @param q - Free-form query text. Whitespace-only or `q.length > 200`
 *   surfaces as a 422 from the backend.
 * @param signal - Optional `AbortSignal`. The CommandPalette uses this
 *   to cancel stale requests while the user types.
 */
export async function searchLibrary(q: string, signal?: AbortSignal): Promise<SearchResponse> {
  const url = new URL("/api/v1/search", window.location.origin);
  url.searchParams.set("q", q);
  const body = await apiFetch(url, signal ? { method: "GET", signal } : { method: "GET" });
  return SearchResponseSchema.parse(body);
}
