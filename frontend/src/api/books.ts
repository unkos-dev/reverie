/**
 * Read-only client for the `/api/books` and `/api/works/{id}` surface.
 *
 * Mirrors the response DTOs in `backend/src/models/library.rs`. Field
 * names are snake_case to match the wire shape — no `serde(rename)` on
 * the backend, no transform on the frontend. The shape is the same
 * source of truth for both sides; if a field is added on the backend,
 * extend the interface here and the type-checker will surface every
 * call site that needs to handle it.
 *
 * Pagination follows the convention documented in the JSON-API ADR:
 * the server emits both an RFC 8288 `Link: <…>; rel="next"` header and
 * an in-body `next_cursor` for convenience. This module reads the
 * in-body field — react-query's `getNextPageParam` consumes it.
 */
import { apiFetch } from "./fetch";

/** Ingestion lifecycle state. Matches `backend/src/models/ingestion_status.rs`. */
export type IngestionStatus = "pending" | "processing" | "complete" | "failed" | "skipped";

/** Enrichment lifecycle state. Matches `backend/src/models/enrichment_status.rs`. */
export type EnrichmentStatus = "pending" | "in_progress" | "complete" | "failed" | "skipped";

/**
 * Series membership for a manifestation. `position` is nullable —
 * a work can sit on a series without a known ordinal.
 */
export interface SeriesRef {
  id: string;
  name: string;
  position: number | null;
}

/**
 * One row of a paginated book list response. Mirrors
 * [`BookListRow`](backend/src/models/library.rs) on the wire.
 */
export interface BookListItem {
  id: string;
  work_id: string;
  title: string;
  authors: string[];
  series: SeriesRef | null;
  isbn_13: string | null;
  cover_url: string;
  ingestion_status: IngestionStatus;
  /** Raw DB string — reconciled enum lands in a follow-up; see `BookListRow::validation_status`. */
  validation_status: string;
  enrichment_status: EnrichmentStatus;
}

/** Envelope returned by `GET /api/books`. `next_cursor === null` means end-of-list. */
export interface BookListResponse {
  items: BookListItem[];
  next_cursor: string | null;
}

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

/**
 * Counts surfaced on the book-detail Versions tab. Mirrors
 * `MetadataVersionSummary` on the wire.
 */
export interface MetadataVersionSummary {
  pending: number;
  accepted: number;
}

/**
 * `GET /api/books/{id}` response. Carries `BookListItem` fields plus
 * work-level prose and metadata-version counts. Mirrors `BookDetail`.
 */
export interface BookDetail {
  id: string;
  work_id: string;
  title: string;
  authors: string[];
  series: SeriesRef | null;
  description: string | null;
  language: string | null;
  isbn_13: string | null;
  isbn_10: string | null;
  cover_url: string;
  tags: string[];
  ingestion_status: IngestionStatus;
  validation_status: string;
  enrichment_status: EnrichmentStatus;
  metadata_version_summary: MetadataVersionSummary;
  created_at: string;
  updated_at: string;
}

/** One manifestation row embedded in a [`WorkDetail`] response. */
export interface WorkManifestation {
  id: string;
  isbn_13: string | null;
  isbn_10: string | null;
  cover_url: string;
  ingestion_status: IngestionStatus;
  validation_status: string;
  enrichment_status: EnrichmentStatus;
  created_at: string;
}

/** `GET /api/works/{id}` response. Lists every visible manifestation for the work. */
export interface WorkDetail {
  id: string;
  title: string;
  authors: string[];
  description: string | null;
  language: string | null;
  series: SeriesRef | null;
  manifestations: WorkManifestation[];
}

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
  return apiFetch<BookListResponse>(url, signal ? { method: "GET", signal } : { method: "GET" });
}

/**
 * Fetch the detail of a single book (manifestation) by id. RLS-hidden
 * rows resolve to 404 (existence-not-leaked) — callers branch on
 * `ApiError.status === 404`, not on a distinct "forbidden" code.
 */
export async function getBook(id: string, signal?: AbortSignal): Promise<BookDetail> {
  return apiFetch<BookDetail>(
    `/api/books/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
}

/**
 * Fetch a work and the manifestations the current user can see for it.
 * Returns 404 when no manifestation is visible (existence-not-leaked).
 */
export async function getWork(id: string, signal?: AbortSignal): Promise<WorkDetail> {
  return apiFetch<WorkDetail>(
    `/api/works/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
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
