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
 * Scope is deliberately narrow: only the resource families below
 * (`backend/src/routes/reading.rs`, `backend/src/routes/metadata.rs`,
 * `backend/src/routes/settings/mod.rs`) resolve to a cache key. Every other
 * path, including the shelves reorder PUT, resolves to `null` and is never
 * touched.
 */

const cache = new Map<string, string>();

const READING_PATH = /^\/api\/v1\/books\/([^/]+)\/reading$/;
const METADATA_PATH = /^\/api\/v1\/books\/([^/]+)\/metadata$/;
const SETTINGS_PATH = /^\/api\/v1\/settings$/;

/**
 * Resolve a request path to its ETag cache key, or `null` when the path is
 * not one of the protected resource families.
 *
 * A resource's GET and write share one URI (`/api/v1/books/{id}/metadata`,
 * `/api/v1/settings`), so a single pattern keys both to the same slot.
 */
export function etagKeyForPath(pathname: string): string | null {
  const reading = READING_PATH.exec(pathname);
  if (reading) return `reading:${reading[1]}`;
  const metadata = METADATA_PATH.exec(pathname);
  if (metadata) return `metadata:${metadata[1]}`;
  if (SETTINGS_PATH.test(pathname)) return "settings";
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
