/**
 * Reverie If-Match ETag retention for optimistic-concurrency PATCH surfaces.
 *
 * Mirrors `csrf.ts`'s module-level cache: a single source of truth per
 * protected resource, hydrated by `apiFetch` from every response that
 * carries an `ETag` header and read back by `apiFetch` to inject `If-Match`
 * on that resource's own PATCH. Callers never thread the header by hand,
 * matching the CSRF token's wrapper-level injection rather than a
 * per-callsite `ifMatch` parameter (contrast `shelves.ts`, whose
 * timestamp-derived scheme predates this module and stays untouched).
 *
 * Scope is deliberately narrow: only the two resource families this phase
 * protects (`backend/src/routes/reading.rs`, `backend/src/routes/
 * metadata.rs`) resolve to a cache key. Every other path, including the
 * shelves reorder PUT, resolves to `null` and is never touched.
 */

const cache = new Map<string, string>();

const READING_PATH = /^\/api\/v1\/books\/([^/]+)\/reading$/;
const METADATA_PATH = /^\/api\/v1\/books\/([^/]+)\/metadata$/;

/**
 * Resolve a request path to its ETag cache key, or `null` when the path is
 * not one of the protected resource families.
 *
 * The metadata GET and PATCH share one URI (`/api/v1/books/{id}/metadata`),
 * so a single pattern keys both to the same `metadata:{id}` slot.
 */
export function etagKeyForPath(pathname: string): string | null {
  const reading = READING_PATH.exec(pathname);
  if (reading) return `reading:${reading[1]}`;
  const metadata = METADATA_PATH.exec(pathname);
  if (metadata) return `metadata:${metadata[1]}`;
  return null;
}

/** Record the most recently seen ETag for a resource key. */
export function rememberEtag(key: string, etag: string): void {
  cache.set(key, etag);
}

/** Read the most recently seen ETag for a resource key, or `null` if none. */
export function getRememberedEtag(key: string): string | null {
  return cache.get(key) ?? null;
}

/**
 * Test-only escape hatch — discard every retained ETag. Production code
 * does NOT call this; the cache lives for the page's lifetime.
 */
export function __resetEtagCacheForTesting(): void {
  cache.clear();
}
