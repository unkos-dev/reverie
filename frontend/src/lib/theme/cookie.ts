/**
 * Cookie name carrying the user's theme preference (system / light / dark).
 * Read both from the pre-paint FOUC script and from `ThemeProvider` to seed
 * synchronous initial state without round-tripping the backend.
 *
 * Cross-stack constant: a drift between any of the three call sites breaks
 * either the FOUC (wrong attribute applied before React boots) or the
 * cross-tab broadcast invariant. UNK-105 tracks the trio.
 */
// Cookie name for the FOUC theme preference. Duplicated in:
//   - backend/src/auth/theme_cookie.rs (THEME_COOKIE_NAME const)
//   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
// All three MUST agree. Tracked as instance 1 under UNK-105.
export const THEME_COOKIE_NAME = "reverie_theme";

// ONE YEAR in seconds. MUST equal `Duration::days(365).whole_seconds()` on the
// backend side (365 × 86400 = 31_536_000). If this constant changes, the
// matching `.max_age(Duration::days(...))` in
// backend/src/auth/theme_cookie.rs MUST change in the same commit.
const ONE_YEAR_SECONDS = 31_536_000;

/**
 * User-facing theme preference. `"system"` is the deferred-to-OS option;
 * the FOUC script and the provider both expand it to a concrete
 * `"light"`/`"dark"` via `matchMedia` for the actual `data-theme` attribute.
 */
export type ThemePreference = "system" | "light" | "dark";

/**
 * Read the FOUC theme cookie. Returns one of the three valid preferences or
 * `null` (cookie absent, malformed, or value outside the strict allowlist).
 * Caller is responsible for collapsing `null` to a default — the current
 * caller (`ThemeProvider.deriveInitialState`) coalesces to `"system"`.
 * Returning `null` rather than a baked-in default keeps the function pure
 * w.r.t. the default choice and lets the FOUC script and React provider
 * each pick their own (both happen to pick `"system"` today).
 *
 * Defensive parse: `document.cookie` is a string the user can edit via
 * devtools or a malicious extension, so the validate-then-narrow walk is
 * deliberate. Avoids `Object.fromEntries(parse(...))` shapes that would
 * pull `cookie`-parsing dependencies into the FOUC-adjacent code path.
 *
 * @returns Stored preference, or `null` when no valid cookie is present.
 */
export function readThemeCookie(): ThemePreference | null {
  const pairs = (document.cookie || "").split("; ");
  for (const pair of pairs) {
    const eq = pair.indexOf("=");
    if (eq === -1) continue;
    if (pair.slice(0, eq) !== THEME_COOKIE_NAME) continue;
    const raw = pair.slice(eq + 1);
    if (raw === "system" || raw === "light" || raw === "dark") return raw;
    return null;
  }
  return null;
}

/**
 * Persist a theme preference as the FOUC cookie. Attribute set is held
 * verbatim equal to the backend's `set_theme_cookie` so a logged-out
 * client can update the cookie without the backend immediately overwriting
 * it on the next request — see the leading comment block for the
 * attribute-by-attribute parity table.
 *
 * Side-effect only: writes `document.cookie`. Browsers silently drop
 * cookie sets that violate any attribute constraint (e.g. `Secure` on
 * an `http://` non-localhost page); callers cannot observe such failures
 * directly. The provider treats the cookie as the source of truth and
 * relies on backend tests to enforce parity.
 *
 * @param value - Preference to persist. The cookie set is unconditional;
 *   callers gate on whether a write is appropriate (no read-modify-write
 *   in this function).
 */
// Attribute parity with backend `set_theme_cookie`:
//   Path=/            — matches backend
//   Max-Age=31536000  — matches backend's Duration::days(365)
//   SameSite=Lax      — matches backend
//   Secure            — always set. Reverie requires HTTPS for any
//                       publicly-reachable deployment (see backend
//                       theme_cookie.rs module header). Browsers accept
//                       Secure cookies on http://localhost as a secure
//                       context, so dev still works.
//   HttpOnly          — intentionally absent on BOTH sides (JS reads for FOUC)
// Drift is caught by the backend `set_theme_cookie_*` tests plus this
// module's `cookie.test.ts` attribute assertions.
export function writeThemeCookie(value: ThemePreference): void {
  document.cookie =
    `${THEME_COOKIE_NAME}=${value}; ` +
    `Path=/; ` +
    `Max-Age=${String(ONE_YEAR_SECONDS)}; ` +
    `SameSite=Lax; ` +
    `Secure`;
}
