/**
 * The browser's record of which account last confirmed a session here.
 *
 * Per-user client caches (the library's first-paint mirror) need a user id
 * before any request has resolved: the route loader reads the mirror
 * synchronously to seed a query key. This hint is that id. It is written
 * wherever `/auth/me` confirms an identity, cleared on sign-out and on the
 * 401 path, and never trusted for anything but cache scoping: it grants
 * nothing, the server's session decides every request.
 *
 * Clearing on session death is what keeps one account's cached
 * presentation from seeding another's first paint on a shared device: with
 * no hint there is no active cache key, and a fresh sign-in re-establishes
 * one before the next library visit. Reads and writes degrade silently,
 * matching the storage modules this scopes.
 */

const LAST_USER_KEY = "reverie_last_user";

/** The last confirmed account id on this browser, or `null` when no session
 *  has been confirmed since the last sign-out. */
export function activeUserId(): string | null {
  try {
    return localStorage.getItem(LAST_USER_KEY);
  } catch {
    return null;
  }
}

/** Record the confirmed account id. Idempotent; called on every identity
 *  confirmation rather than only at login, so it self-heals. */
export function rememberActiveUser(id: string): void {
  try {
    localStorage.setItem(LAST_USER_KEY, id);
  } catch {
    // Storage unavailable: per-user caches stay cold, nothing breaks.
  }
}

/** Forget the account id, leaving per-user caches in place for the
 *  account's return. Called on sign-out and on the 401 path. */
export function forgetActiveUser(): void {
  try {
    localStorage.removeItem(LAST_USER_KEY);
  } catch {
    // Storage unavailable: there was nothing to forget.
  }
}
