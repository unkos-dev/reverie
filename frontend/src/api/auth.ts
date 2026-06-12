/**
 * Client for the session-lifecycle `/auth/*` endpoints.
 *
 * THREAT: logout must actually destroy the server-side session — a
 * client that merely drops UI state leaves a live session cookie
 * replayable from the browser. The POST goes through {@link apiFetch}
 * so the CSRF synchronizer token rides along on the unsafe verb once
 * the enforcement middleware lands (token issuance shipped first; see
 * the csrf-rollout ADR).
 *
 * `/auth/me` reads live in `hooks/useAuthMe.ts` (query-shaped); this
 * module owns the imperative mutations only.
 */
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
