/**
 * Client for the session-lifecycle `/auth/*` endpoints.
 *
 * THREAT: logout must actually destroy the server-side session — a
 * client that merely drops UI state leaves a live session cookie
 * replayable from the browser. The POST goes through {@link apiFetch}
 * so the CSRF synchronizer token rides along on the unsafe verb once
 * the enforcement middleware lands (token issuance shipped first; see
 * the CSRF defense section of `adr/2026-05-22-json-api-conventions.md`).
 *
 * `/auth/me` reads live in `hooks/useAuthMe.ts` (query-shaped); this
 * module owns the imperative mutations only.
 */
import { z } from "zod";

import { refreshCsrfToken } from "./csrf";
import { apiFetch } from "./fetch";

/**
 * Destroy the current session (`POST /auth/logout`, idempotent 204).
 *
 * # Errors
 * Throws {@link ApiError} on a non-2xx response and `TypeError` on
 * network failure — callers decide whether to hard-navigate anyway.
 */
export async function logout(): Promise<void> {
  await apiFetch("/auth/logout", { method: "POST" });
}

/** Public first-run / provider state from `GET /auth/setup/status`. */
const SetupStatusSchema = z.object({
  setup_required: z.boolean(),
  local_auth_enabled: z.boolean(),
  oidc_enabled: z.boolean(),
});

/** First-run and provider state used by the auth screens and the redirect. */
export type SetupStatus = z.infer<typeof SetupStatusSchema>;

/**
 * Fetch the public setup/provider state (`GET /auth/setup/status`). Drives the
 * provider-aware redirect and which auth screen the SPA shows.
 *
 * # Errors
 * Throws {@link ApiError} on a non-2xx response; throws `ZodError` if the body
 * does not match the schema.
 */
export async function fetchSetupStatus(signal?: AbortSignal): Promise<SetupStatus> {
  return SetupStatusSchema.parse(await apiFetch("/auth/setup/status", { signal }));
}

/**
 * Sign in with email + password (`POST /auth/local/login`). On success the
 * session carries a fresh CSRF token, so this hydrates the client token cache
 * before the caller issues any mutating request.
 *
 * # Errors
 * Throws {@link ApiError} on invalid credentials (422), rate limiting (429), or
 * disabled local auth (404).
 */
export async function loginLocal(email: string, password: string): Promise<void> {
  await apiFetch("/auth/local/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
  await refreshCsrfToken();
}

/**
 * Create the first administrator (`POST /auth/setup`). Does not establish a
 * session: the caller signs in afterwards.
 *
 * # Errors
 * Throws {@link ApiError} when setup is already complete (409) or validation
 * fails (422).
 */
export async function setupAdmin(
  email: string,
  displayName: string,
  password: string,
): Promise<void> {
  await apiFetch("/auth/setup", {
    method: "POST",
    body: JSON.stringify({ email, display_name: displayName, password }),
  });
}

/**
 * Start password recovery (`POST /auth/forgot-password`). Always succeeds
 * generically; the operator reads the issued PIN from the host file.
 *
 * # Errors
 * Throws {@link ApiError} on rate limiting (429) or disabled local auth (404).
 */
export async function requestPasswordReset(email: string): Promise<void> {
  await apiFetch("/auth/forgot-password", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

/**
 * Complete password recovery with a PIN (`POST /auth/reset-password`). Does not
 * establish a session: the caller signs in with the new password.
 *
 * # Errors
 * Throws {@link ApiError} on an invalid or expired request (422) or rate
 * limiting (429).
 */
export async function resetPassword(
  email: string,
  pin: string,
  newPassword: string,
): Promise<void> {
  await apiFetch("/auth/reset-password", {
    method: "POST",
    body: JSON.stringify({ email, pin, new_password: newPassword }),
  });
}
