import type { ThemePreference } from "./cookie";

/**
 * Result of `fetchMe()`. The provider uses this to reconcile cookie-derived
 * state with the server's record of `theme_preference` for the logged-in
 * user.
 *
 * `unauthenticated` is the documented happy path for a logged-out visitor:
 * the provider stays on the cookie-derived preference and skips the PATCH.
 */
export type MeResult =
  | { kind: "ok"; theme_preference: ThemePreference }
  | { kind: "unauthenticated" }
  | { kind: "error"; status: number };

/**
 * Fetch the authenticated user's record (currently only the
 * `theme_preference` field is consumed). The discriminated `MeResult` lets
 * callers distinguish "logged out" (401 → `unauthenticated`) from "broken
 * request" (other non-2xx, or 200 with a malformed body → `error`).
 *
 * Treats `unauthenticated` as a normal outcome, not an error, because the
 * provider's reconcile path runs unconditionally on mount: throwing for
 * anonymous visitors would flood console.warn and noise out real failures.
 *
 * @param signal - Optional abort signal; passed through to `fetch`.
 *   Caller is responsible for wiring up `AbortController` (see
 *   ThemeProvider's reconcile effect).
 * @returns `ok` with the validated preference, `unauthenticated` on 401,
 *   or `error` with the response status (also returned when the body's
 *   `theme_preference` is missing or unrecognised — schema drift would
 *   otherwise read as a successful fetch).
 */
export async function fetchMe(signal?: AbortSignal): Promise<MeResult> {
  const opts: RequestInit = {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    ...(signal ? { signal } : {}),
  };
  const resp = await fetch("/auth/me", opts);
  if (resp.status === 401) return { kind: "unauthenticated" };
  if (!resp.ok) return { kind: "error", status: resp.status };
  const body = (await resp.json()) as { theme_preference?: unknown };
  const value = body.theme_preference;
  if (value === "system" || value === "light" || value === "dark") {
    return { kind: "ok", theme_preference: value };
  }
  return { kind: "error", status: 200 };
}

/**
 * Persist the user's theme preference server-side. Fire-and-observe shape:
 * the returned object surfaces only `ok` and `status` so the caller can
 * branch between "rolled back, surface a toast" (4xx non-401) and "keep the
 * optimistic update, log a warning" (401 / 5xx). Deliberate omission of
 * the response body — the provider's optimistic update is the source of
 * truth and a re-read would race with the in-flight cookie write.
 *
 * @param value - Preference to persist; same allowlist the cookie carries.
 * @param signal - Optional abort signal; pass through from caller's
 *   AbortController.
 * @returns `{ ok, status }` extracted from the response. Network failures
 *   throw (caller branches via try/catch, see ThemeProvider's
 *   `setPreference`).
 */
export async function patchTheme(
  value: ThemePreference,
  signal?: AbortSignal,
): Promise<{ ok: boolean; status: number }> {
  const opts: RequestInit = {
    method: "PATCH",
    credentials: "same-origin",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ theme_preference: value }),
    ...(signal ? { signal } : {}),
  };
  const resp = await fetch("/auth/me/theme", opts);
  return { ok: resp.ok, status: resp.status };
}
