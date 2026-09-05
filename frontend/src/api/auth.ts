/**
 * Client for the session-lifecycle `/auth/*` endpoints.
 *
 * THREAT: logout must actually destroy the server-side session — a
 * client that merely drops UI state leaves a live session cookie
 * replayable from the browser. The POST goes through {@link apiFetch}
 * so the CSRF synchronizer token rides along on the unsafe verb once
 * the enforcement middleware lands (token issuance shipped first; see
 * `docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md`).
 *
 * `/auth/me` reads live in `hooks/useAuthMe.ts` (query-shaped); this
 * module owns the imperative mutations only.
 */
import { z } from "zod";

import {
  ChangePasswordSchema,
  ForgotPasswordSchema,
  LoginLocalSchema,
  RegisterSchema,
  ResetPasswordSchema,
  SetupAdminSchema,
} from "./auth.schemas";
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
  const body = LoginLocalSchema.parse({ email, password });
  await apiFetch("/auth/local/login", {
    method: "POST",
    body: JSON.stringify(body),
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
  const body = SetupAdminSchema.parse({ email, display_name: displayName, password });
  await apiFetch("/auth/setup", {
    method: "POST",
    body: JSON.stringify(body),
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
  const body = ForgotPasswordSchema.parse({ email });
  await apiFetch("/auth/forgot-password", {
    method: "POST",
    body: JSON.stringify(body),
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
  const body = ResetPasswordSchema.parse({ email, pin, new_password: newPassword });
  await apiFetch("/auth/reset-password", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

/**
 * Self-register an account (`POST /auth/register`). Config-gated; always creates
 * an adult, never an admin or child. Does not establish a session: the caller
 * signs in afterwards.
 *
 * # Errors
 * Throws {@link ApiError} when registration or local auth is disabled (404),
 * the email is already in use (409), validation or policy fails (422), or the
 * per-source limit is exceeded (429).
 */
export async function register(
  email: string,
  displayName: string,
  password: string,
): Promise<void> {
  const body = RegisterSchema.parse({ email, display_name: displayName, password });
  await apiFetch("/auth/register", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

/**
 * Change the caller's own password (`POST /api/v1/account/password`). Verifies
 * the current password and invalidates every session, so the caller signs in
 * again afterwards.
 *
 * # Errors
 * Throws {@link ApiError} when the current password is wrong, the new password
 * fails the policy, or there is no local credential (422).
 */
export async function changeOwnPassword(
  currentPassword: string,
  newPassword: string,
): Promise<void> {
  const body = ChangePasswordSchema.parse({
    current_password: currentPassword,
    new_password: newPassword,
  });
  await apiFetch("/api/v1/account/password", {
    method: "POST",
    body: JSON.stringify(body),
  });
}
