/**
 * Reverie CSRF synchronizer-token reader.
 *
 * THREAT: `SameSite=Lax` cookies alone don't cover top-level GET CSRF
 * returning sensitive state and don't protect when a cookie is set with
 * `SameSite=None`. The OWASP synchronizer token bound to the user's
 * session is the primary defense; this module owns the client side of
 * that defense (the `csrf_required` tower middleware owns the server
 * side; `apiFetch` owns header injection on mutating verbs). See
 * `adr/2026-05-22-json-api-conventions.md` §"CSRF defense".
 *
 * The token lives in module-level state, a single source of truth for
 * every request in the SPA. Hydration paths: `apiFetch` refreshes the
 * cache lazily before its first mutating request when the cache is
 * empty (an OIDC session's only hydration path), and once more after a
 * `403 csrf-mismatch` or `428 csrf-missing` response so a rotated or
 * expired token recovers without a full page reload; `loginLocal`
 * hydrates eagerly after a local password sign-in.
 */

import { z } from "zod";

/** Internal cache. `null` = never hydrated or last refresh failed. */
let cachedToken: string | null = null;

/**
 * Minimal `/auth/me` body subset this module consumes. Other fields
 * (`id`, `role`, `theme_preference`, …) are read by their own owners
 * (`ThemeProvider`, auth boundary) — keeping the schema narrow here
 * avoids coupling the CSRF reader to unrelated server changes.
 */
const MeBodyShape = z.object({
  // `.min(1)` rejects empty strings; an empty token would otherwise
  // populate the cache and `apiFetch` would inject a blank
  // `X-CSRF-Token` header, which the middleware would treat as a
  // mismatch on every mutating request. Empty must funnel into the
  // same null-cache path as `null`, omitted, and schema-drift cases.
  csrf_token: z.string().min(1).nullish(),
});

/**
 * Read the cached CSRF token without making a network request.
 *
 * Returns `null` until `refreshCsrfToken()` has hydrated the cache.
 * Callers that need a guaranteed-fresh value (`apiFetch` on
 * first mutating verb, or after a `403 csrf-mismatch`) call
 * `refreshCsrfToken()` first and await the result.
 *
 * @returns The cached base64url-unpadded token (43 chars), or `null`
 *   when the cache has never been hydrated, the user was
 *   unauthenticated on the last `/auth/me`, or the response omitted
 *   the field. Callers MUST treat `null` as "do not send the
 *   `X-CSRF-Token` header" — sending an empty header is worse than
 *   sending none.
 */
export function getCsrfToken(): string | null {
  return cachedToken;
}

/**
 * Fetch `/auth/me`, extract `csrf_token`, and update the module-level
 * cache. Idempotent — safe to call from multiple consumers; the last
 * successful response wins.
 *
 * Failure model (matches `ThemeProvider`'s tolerance): a non-OK status
 * or schema mismatch clears the cache to `null` rather than throwing.
 * The caller (`apiFetch`) then refuses to inject the header
 * and surfaces the upstream auth/network failure on the actual
 * mutating request, not here. Throwing would force every consumer to
 * carry a try/catch around an essentially-best-effort hydration call.
 *
 * @param signal - Optional abort signal; pass through from caller's
 *   `AbortController`. Wired identically to `fetchMe()` in
 *   `lib/theme/api.ts`.
 * @returns The fresh token on success, or `null` on any failure
 *   (network error, 401, malformed body, `csrf_token === null`). The
 *   return value mirrors the post-refresh cache, so callers can chain
 *   `await refreshCsrfToken()` without a follow-up `getCsrfToken()`.
 */
export async function refreshCsrfToken(signal?: AbortSignal): Promise<string | null> {
  const opts: RequestInit = {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    ...(signal ? { signal } : {}),
  };
  let resp: Response;
  try {
    resp = await fetch("/auth/me", opts);
  } catch {
    cachedToken = null;
    return null;
  }
  if (!resp.ok) {
    cachedToken = null;
    return null;
  }
  let body: unknown;
  try {
    body = await resp.json();
  } catch {
    cachedToken = null;
    return null;
  }
  const parsed = MeBodyShape.safeParse(body);
  if (!parsed.success) {
    cachedToken = null;
    return null;
  }
  // `nullish` ↦ `string | null | undefined`. The backend sends the
  // string for OIDC-authed sessions and `null` for Basic-auth sessions
  // (OPDS); the field is always present. Defensive treatment of
  // `undefined` keeps the reader robust if the field is ever omitted
  // (e.g. a future fork strips it from a public endpoint).
  cachedToken = parsed.data.csrf_token ?? null;
  return cachedToken;
}

/**
 * Test-only escape hatch — discard the cached token. Production code
 * does NOT call this; the cache lives for the page's lifetime and is
 * reset implicitly when the SPA reloads.
 *
 * Module-level mutable state is unavoidable here (the cache is the
 * point of the module), so tests need a way to reset between cases
 * without re-importing the module via `vi.resetModules()`.
 */
export function __resetCsrfTokenForTesting(): void {
  cachedToken = null;
}

/**
 * Test-only escape hatch — seed the cached token directly. The global
 * test setup uses this so suites exercising mutating API calls start
 * from the authenticated steady state (token already hydrated) instead
 * of each triggering `apiFetch`'s lazy first-use hydration; the
 * hydration behaviour itself is pinned by `fetch.test.ts`, which resets
 * the cache explicitly.
 */
export function __seedCsrfTokenForTesting(token: string): void {
  cachedToken = token;
}
