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
