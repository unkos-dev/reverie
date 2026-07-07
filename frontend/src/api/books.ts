/**
 * Read-only client for the `/api/v1/books` and `/api/v1/works/{id}` surface.
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

const ValidationStatusSchema = z.enum(["pending", "clean", "repaired", "degraded", "failed"]);
/** Validation lifecycle state. Matches `backend/src/models/validation_status.rs`. */
export type ValidationStatus = z.infer<typeof ValidationStatusSchema>;

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

const ReadingStatusSchema = z.enum(["want_to_read", "reading", "on_hold", "finished", "abandoned"]);
/** Per-user reading lifecycle state. Matches `backend/src/models/reading_status.rs`. */
export type ReadingStatus = z.infer<typeof ReadingStatusSchema>;

export { ReadingStatusSchema };

const ReadingStateSummarySchema = z.object({
  status: ReadingStatusSchema.nullable(),
  rating: z.number().int().nullable(),
  progress_pct: z.number().nullable(),
});
/**
 * Caller-scoped reading-state summary batch-loaded onto list rows.
 * Mirrors `ReadingStateSummary` in `backend/src/models/reading_state.rs`;
 * `null` when the caller has no reading-state row for the book.
 */
export type ReadingStateSummary = z.infer<typeof ReadingStateSummarySchema>;

const BookListItemSchema = z.object({
  id: z.string(),
  work_id: z.string(),
  title: z.string(),
  subtitle: z.string().nullable(),
  authors: z.array(z.string()),
  series: SeriesRefSchema.nullable(),
  isbn_13: z.string().nullable(),
  pages: z.number().int().nullable(),
  cover_url: z.string(),
  ingestion_status: IngestionStatusSchema,
  validation_status: ValidationStatusSchema,
  enrichment_status: EnrichmentStatusSchema,
  reading_state: ReadingStateSummarySchema.nullable(),
  created_at: z.string(),
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
/** Envelope returned by `GET /api/v1/books`. `next_cursor === null` means end-of-list. */
export type BookListResponse = z.infer<typeof BookListResponseSchema>;

const SORT_FIELDS = ["title", "author", "created_at", "pages"] as const;

/** Sortable column names accepted by `GET /api/v1/books?sort=…`. */
export type SortField = (typeof SORT_FIELDS)[number];

/** One level of a sort stack: a wire field name plus its direction. */
export type SortLevelParam = { field: SortField; desc: boolean };

/** Server-side cap on stack length; `SortSpec::parse` enforces the same limit. */
export const MAX_SORT_LEVELS = 3;

function isSortField(value: string): value is SortField {
  return (SORT_FIELDS as readonly string[]).includes(value);
}

/**
 * Parse a `?sort=` value into an ordered stack. Tolerant of stale or
 * hand-crafted input: unknown fields and repeated columns are dropped
 * silently, the stack is capped at {@link MAX_SORT_LEVELS}, and an
 * all-invalid or empty input degrades to `[]` (the server's default order).
 */
export function parseSortParam(raw: string): SortLevelParam[] {
  const levels: SortLevelParam[] = [];
  const seen = new Set<SortField>();
  for (const token of raw.split(",")) {
    const desc = token.startsWith("-");
    const field = desc ? token.slice(1) : token;
    if (!isSortField(field) || seen.has(field)) continue;
    seen.add(field);
    levels.push({ field, desc });
    if (levels.length === MAX_SORT_LEVELS) break;
  }
  return levels;
}

/**
 * Serialize a sort stack into the JSON:API `?sort=` wire string (`-` prefix
 * for descending). Inverse of {@link parseSortParam} for a stack that
 * already passed through it.
 */
export function serializeSortParam(levels: readonly SortLevelParam[]): string {
  return levels.map((level) => (level.desc ? `-${level.field}` : level.field)).join(",");
}

/** Query parameters for `GET /api/v1/books`. Every field is optional. */
export interface ListBooksParams {
  cursor?: string;
  author?: string;
  series?: string;
  shelf?: string;
  q?: string;
  /** Pre-validated JSON:API sort string; build via {@link serializeSortParam}. */
  sort?: string;
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

const MetadataVersionRowSchema = z.object({
  id: z.string(),
  field_name: z.string(),
  source: z.string(),
  new_value: z.unknown(),
  status: z.string(),
  confidence_score: z.number(),
  match_type: z.string(),
  observation_count: z.number().int(),
});
/**
 * One pending draft surfaced on the Versions tab. Mirrors
 * `MetadataVersionRow` in `backend/src/models/library.rs`.
 *
 * `status` is always `"pending"` here — promotion lives on canonical
 * pointer columns, not on this enum. Promoted rows are filtered server-
 * side so the same id never appears twice.
 *
 * `new_value` is untyped JSON because field-specific shape varies
 * (string for `title`, ISO date string for `pub_date`); the consumer
 * narrows per `field_name` at the render site.
 */
export type MetadataVersionRow = z.infer<typeof MetadataVersionRowSchema>;

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
  publisher: z.string().nullable(),
  pub_date: z.string().nullable(),
  cover_url: z.string(),
  tags: z.array(z.string()),
  ingestion_status: IngestionStatusSchema,
  validation_status: ValidationStatusSchema,
  enrichment_status: EnrichmentStatusSchema,
  metadata_version_summary: MetadataVersionSummarySchema,
  metadata_versions: z.array(MetadataVersionRowSchema),
  created_at: z.string(),
  updated_at: z.string(),
});
/**
 * `GET /api/v1/books/{id}` response. Carries `BookListItem` fields plus
 * work-level prose and metadata-version counts. Mirrors `BookDetail`.
 */
export type BookDetail = z.infer<typeof BookDetailSchema>;

const WorkManifestationSchema = z.object({
  id: z.string(),
  isbn_13: z.string().nullable(),
  isbn_10: z.string().nullable(),
  cover_url: z.string(),
  ingestion_status: IngestionStatusSchema,
  validation_status: ValidationStatusSchema,
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
/** `GET /api/v1/works/{id}` response. Lists every visible manifestation for the work. */
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
  const url = buildUrl("/api/v1/books", {
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
    `/api/v1/books/${encodeURIComponent(id)}`,
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
    `/api/v1/works/${encodeURIComponent(id)}`,
    signal ? { method: "GET", signal } : { method: "GET" },
  );
  return WorkDetailSchema.parse(body);
}

/**
 * Per-role contributor replace/clear, nested under `contributors`.
 * Absent role = untouched; `null` or `[]` clears the role; a non-empty
 * array replaces it wholesale in the given order. Strict: the backend
 * rejects unknown role keys with 422.
 */
const ContributorsPatchSchema = z.strictObject({
  author: z.array(z.string()).nullable().optional(),
  editor: z.array(z.string()).nullable().optional(),
  translator: z.array(z.string()).nullable().optional(),
});

const UpdateBookMetadataFieldsSchema = z.object({
  title: z.string().nullable().optional(),
  subtitle: z.string().nullable().optional(),
  description: z.string().nullable().optional(),
  language: z.string().nullable().optional(),
  publisher: z.string().nullable().optional(),
  pub_date: z.string().nullable().optional(),
  isbn_10: z.string().nullable().optional(),
  isbn_13: z.string().nullable().optional(),
  pages: z.number().int().positive().nullable().optional(),
  contributors: ContributorsPatchSchema.nullable().optional(),
});
/**
 * RFC 7396 JSON Merge Patch body for `PATCH /api/v1/books/{id}/metadata`.
 *
 * Each field value distinguishes three states:
 * * key omitted → field unchanged
 * * key present, value non-`null` → set field to that value
 * * key present, value = `null` → clear the canonical column
 *
 * The backend types ISBN / pub_date strings; per-field parsing happens
 * server-side. `title` cannot be cleared (canonical title is NOT NULL
 * on `works`); the server returns 422 if the request body sets `title:
 * null`.
 *
 * The exported {@link UpdateBookMetadataFieldsSchema} is the runtime
 * boundary guard — form callers parse user-assembled bodies through it
 * before passing to {@link updateBookMetadata}, per the boundary-
 * validation rule in `frontend/CLAUDE.md`.
 */
export type UpdateBookMetadataFields = z.infer<typeof UpdateBookMetadataFieldsSchema>;

export { UpdateBookMetadataFieldsSchema };

const FieldVersionChangeSchema = z.object({
  value: z.unknown().nullable(),
  version_id: z.uuid().nullable(),
  previous_version_id: z.uuid().nullable(),
});
/**
 * One field's outcome from a metadata PATCH. `value` is the canonical
 * value as applied, after server-side normalization (e.g. ISBN
 * checksum), null when the field was cleared. `version_id` is the
 * journal row now wired as canonical, null when cleared to no
 * version. `previous_version_id` is the pointer that was canonical
 * before this patch, null when the field was previously unset or has
 * no version pointer of its own (vocabulary fields).
 */
export type FieldVersionChange = z.infer<typeof FieldVersionChangeSchema>;

export const UpdateMetadataResponseSchema = z.object({
  fields: z.record(z.string(), FieldVersionChangeSchema),
});
/**
 * `PATCH /api/v1/books/{id}/metadata` response body. Keyed by patched
 * field name, including `contributors.<role>` style keys, so a
 * caller can diff the edited column and drive undo without a
 * positional lookup. Mirrors `UpdateMetadataResponse` on the wire.
 */
export type UpdateMetadataResponse = z.infer<typeof UpdateMetadataResponseSchema>;

/**
 * Manually edit canonical metadata for a book. Each touched field
 * lands as a new `metadata_versions` row (`source = 'manual'`) and the
 * canonical pointer is rewired in the same transaction. Pending AI/OPF
 * drafts on the same field are NOT auto-rejected; operators can
 * revert to them later via `revertField`.
 *
 * Throws an `ApiError` with `status === 422` when the body has no
 * populated fields, or with `status === 403` for child accounts.
 *
 * The request body is a bare RFC 7396 JSON Merge Patch, no `{fields}`
 * envelope. On success the server returns 200 with the per-field
 * applied values and version ids; callers that only need the
 * side-effect can ignore the return value. Callers should still
 * invalidate the `["books", "detail", id]` query key so the Versions
 * tab + canonical fields refetch.
 */
export async function updateBookMetadata(
  id: string,
  fields: UpdateBookMetadataFields,
  signal?: AbortSignal,
): Promise<UpdateMetadataResponse> {
  const init: RequestInit = {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
    ...(signal ? { signal } : {}),
  };
  const body = await apiFetch(`/api/v1/books/${encodeURIComponent(id)}/metadata`, init);
  return UpdateMetadataResponseSchema.parse(body);
}

/**
 * Build a `URL` for an `/api/v1/*` endpoint, dropping `undefined` params.
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
