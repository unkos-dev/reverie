/**
 * Reverie CSRF synchronizer-token reader (Phase 1).
 *
 * THREAT: `SameSite=Lax` cookies alone don't cover top-level GET CSRF
 * returning sensitive state and don't protect when a cookie is set with
 * `SameSite=None`. The OWASP synchronizer token bound to the user's
 * session is the primary defense; this module owns the client side of
 * that defense. See `adr/2026-05-22-json-api-conventions.md` §"CSRF
 * defense" for the order-of-operations split between Phase 1 (this
 * module + `/auth/me` field + backend token issuance) and Phase 2
 * (`csrf_required` tower middleware + `apiFetch` header injection).
 *
 * The token lives in module-level state — single source of truth for
 * every request in the SPA. Hydration is on-demand via
 * `refreshCsrfToken()`; the Phase-2 `apiFetch` wrapper will (a) call
 * `refreshCsrfToken()` once during app boot before any mutating verb
 * is issued, and (b) re-call it once on a `403 csrf-mismatch` response
 * so a rotated token (Phase-2 role-change rotation) recovers without
 * a full page reload.
 *
 * Out of scope for Phase 1: header injection on mutating verbs (that
 * is `apiFetch`'s job, landing in 11a Task 11).
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
  // populate the cache and Phase 2's `apiFetch` would inject a blank
  // `X-CSRF-Token` header, which the middleware would treat as a
  // mismatch on every mutating request. Empty must funnel into the
  // same null-cache path as `null`, omitted, and schema-drift cases.
  csrf_token: z.string().min(1).nullish(),
});

/**
 * Read the cached CSRF token without making a network request.
 *
 * Returns `null` until `refreshCsrfToken()` has hydrated the cache.
 * Callers that need a guaranteed-fresh value (Phase-2 `apiFetch` on
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
 * The caller (Phase-2 `apiFetch`) then refuses to inject the header
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
  // `nullish` ↦ `string | null | undefined`. Phase-1 backend sends the
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
