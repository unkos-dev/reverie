/**
 * Read-only client for the `/api/books` and `/api/works/{id}` surface.
 *
 * Mirrors the response DTOs in `backend/src/models/library.rs`. Field
 * names are snake_case to match the wire shape — no `serde(rename)` on
 * the backend, no transform on the frontend. The shape is the same
 * source of truth for both sides; if a field is added on the backend,
 * extend the schema here and the type-checker will surface every call
 * site that needs to handle it.
 *
 * **Runtime validation at the boundary.** Per `frontend/CLAUDE.md`,
 * every response body is parsed through a Zod schema before being
 * returned to the caller — `apiFetch` returns `unknown`, and the
 * schemas here convert `unknown` into the strongly-typed shapes the
 * UI consumes. A schema-violating response surfaces as a `ZodError`
 * rather than silently corrupting downstream state.
 *
 * Pagination follows the convention documented in the JSON-API ADR:
 * the server emits both an RFC 8288 `Link: <…>; rel="next"` header
 * and an in-body `next_cursor` for convenience. This module reads the
 * in-body field — react-query's `getNextPageParam` consumes it.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const IngestionStatusSchema = z.enum(["pending", "processing", "complete", "failed", "skipped"]);
/** Ingestion lifecycle state. Matches `backend/src/models/ingestion_status.rs`. */
export type IngestionStatus = z.infer<typeof IngestionStatusSchema>;

const EnrichmentStatusSchema = z.enum(["pending", "in_progress", "complete", "failed", "skipped"]);
/** Enrichment lifecycle state. Matches `backend/src/models/enrichment_status.rs`. */
export type EnrichmentStatus = z.infer<typeof EnrichmentStatusSchema>;

const SeriesRefSchema = z.object({
  id: z.string(),
  name: z.string(),
  position: z.number().nullable(),
});
/**
 * Series membership for a manifestation. `position` is nullable —
 * a work can sit on a series without a known ordinal.
 */
export type SeriesRef = z.infer<typeof SeriesRefSchema>;

const BookListItemSchema = z.object({
  id: z.string(),
  work_id: z.string(),
  title: z.string(),
  authors: z.array(z.string()),
  series: SeriesRefSchema.nullable(),
  isbn_13: z.string().nullable(),
  cover_url: z.string(),
  ingestion_status: IngestionStatusSchema,
  // Raw DB string — reconciled enum lands in a follow-up; backend
  // docstring on `BookListRow::validation_status` notes the
  // pending|valid|repaired|degraded variants.
  validation_status: z.string(),
  enrichment_status: EnrichmentStatusSchema,
});
/**
 * One row of a paginated book list response. Mirrors
 * [`BookListRow`](backend/src/models/library.rs) on the wire.
 */
export type BookListItem = z.infer<typeof BookListItemSchema>;

const BookListResponseSchema = z.object({
  items: z.array(BookListItemSchema),
  next_cursor: z.string().nullable(),
});
/** Envelope returned by `GET /api/books`. `next_cursor === null` means end-of-list. */
export type BookListResponse = z.infer<typeof BookListResponseSchema>;

/** Sort modes accepted by `GET /api/books?sort=…`. */
export type ListSort = "recent" | "title" | "author";

/** Query parameters for `GET /api/books`. Every field is optional. */
export interface ListBooksParams {
  cursor?: string;
  author?: string;
  series?: string;
  shelf?: string;
  q?: string;
  sort?: ListSort;
}

const MetadataVersionSummarySchema = z.object({
  pending: z.number().int().nonnegative(),
  accepted: z.number().int().nonnegative(),
});
/**
 * Counts surfaced on the book-detail Versions tab. Mirrors
 * `MetadataVersionSummary` on the wire.
 */
export type MetadataVersionSummary = z.infer<typeof MetadataVersionSummarySchema>;

const BookDetailSchema = z.object({
  id: z.string(),
  work_id: z.string(),
  title: z.string(),
  authors: z.array(z.string()),
  series: SeriesRefSchema.nullable(),
  description: z.string().nullable(),
  language: z.string().nullable(),
  isbn_13: z.string().nullable(),
  isbn_10: z.string().nullable(),
  cover_url: z.string(),
  tags: z.array(z.string()),
  ingestion_status: IngestionStatusSchema,
  validation_status: z.string(),
  enrichment_status: EnrichmentStatusSchema,
  metadata_version_summary: MetadataVersionSummarySchema,
  created_at: z.string(),
  updated_at: z.string(),
});
/**
 * `GET /api/books/{id}` response. Carries `BookListItem` fields plus
 * work-level prose and metadata-version counts. Mirrors `BookDetail`.
 */
export type BookDetail = z.infer<typeof BookDetailSchema>;

const WorkManifestationSchema = z.object({
  id: z.string(),
  isbn_13: z.string().nullable(),
  isbn_10: z.string().nullable(),
  cover_url: z.string(),
  ingestion_status: IngestionStatusSchema,
  validation_status: z.string(),
  enrichment_status: EnrichmentStatusSchema,
  created_at: z.string(),
});
/** One manifestation row embedded in a [`WorkDetail`] response. */
export type WorkManifestation = z.infer<typeof WorkManifestationSchema>;

const WorkDetailSchema = z.object({
  id: z.string(),
  title: z.string(),
  authors: z.array(z.string()),
  description: z.string().nullable(),
  language: z.string().nullable(),
  series: SeriesRefSchema.nullable(),
  manifestations: z.array(WorkManifestationSchema),
});
/** `GET /api/works/{id}` response. Lists every visible manifestation for the work. */
export type WorkDetail = z.infer<typeof WorkDetailSchema>;

/**
 * Fetch a paginated page of the user's library. `cursor` is opaque
 * base64url — pass back the previous response's `next_cursor` to
 * advance. Empty `next_cursor === null` marks end-of-list.
 *
 * @param params - Filters / sort / cursor. Undefined fields are
 *   omitted from the URL so the backend applies its defaults.
 * @param signal - Optional `AbortSignal` to cancel in-flight requests.
 */
export async function listBooks(
  params: ListBooksParams = {},
  signal?: AbortSignal,
): Promise<BookListResponse> {
  const url = buildUrl("/api/books", {
    cursor: params.cursor,
    author: params.author,
    series: params.series,
    shelf: params.shelf,
    q: params.q,
    sort: params.sort,
  });
  const body = await apiFetch(url, signal ? { method: "GET", signal } : { method: "GET" });
  return BookListResponseSchema.parse(body);
}

/**
 * Fetch the detail of a single book (manifestation) by id. RLS-hidden
 * rows resolve to 404 (existence-not-leaked) — callers branch on
 * `ApiError.status === 404`, not on a distinct "forbidden" code.
 */
export async function getBook(id: string, signal?: AbortSignal): Promise<BookDetail> {
  const body = await apiFetch(
    `/api/books/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
  return BookDetailSchema.parse(body);
}

/**
 * Fetch a work and the manifestations the current user can see for it.
 * Returns 404 when no manifestation is visible (existence-not-leaked).
 */
export async function getWork(id: string, signal?: AbortSignal): Promise<WorkDetail> {
  const body = await apiFetch(
    `/api/works/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
  return WorkDetailSchema.parse(body);
}

/**
 * Build a `URL` for an `/api/*` endpoint, dropping `undefined` params.
 * Uses `window.location.origin` as the base so the URL is parseable;
 * the same-origin prefix is stripped by the proxy/route on the server.
 */
function buildUrl(path: string, params: Record<string, string | undefined>): URL {
  const url = new URL(path, window.location.origin);
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) url.searchParams.set(k, v);
  }
  return url;
}
